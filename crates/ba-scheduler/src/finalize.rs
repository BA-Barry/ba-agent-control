use std::path::PathBuf;

use ba_core::{EntityId, Gate, Receipt, Task, TaskStatus, Worktree, WorktreeStatus, now};
use ba_git::{FinalizedWorktree, RepoPath, WorktreeHandle, WorktreeManager, WriteScope};
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    GateEligibility, SchedulerError, configured_gate, configured_gate_names,
    evaluate_required_gates,
};

#[derive(Debug, Deserialize)]
struct RecordedGateCommand {
    program: String,
    args: Vec<String>,
}

/// Durable result of quality-gated local branch and commit creation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FinalizationResult {
    pub worktree: Option<Worktree>,
    pub git: Option<FinalizedWorktree>,
    pub receipt: Receipt,
}

/// Converts an eligible detached worktree into a local task branch and receipt.
#[derive(Clone, Debug)]
pub struct FinalizationCoordinator {
    store: SqliteStore,
    worktree_root: PathBuf,
}

impl FinalizationCoordinator {
    #[must_use]
    pub fn new(store: SqliteStore, worktree_root: PathBuf) -> Self {
        Self {
            store,
            worktree_root,
        }
    }

    /// Complete a task only after every configured non-empty gate has passed.
    /// Write tasks create a local branch/commit; read tasks create a permanent review receipt.
    /// Incomplete gates and already finalized tasks return `None`.
    ///
    /// # Errors
    ///
    /// Returns an error when durable metadata is inconsistent, changed files escape
    /// the declared scope, or Git cannot create the local branch and commit.
    pub fn finalize_if_eligible(
        &self,
        task_id: &EntityId,
        run_id: Option<&EntityId>,
    ) -> Result<Option<FinalizationResult>, SchedulerError> {
        let task = self
            .store
            .get_task(task_id)?
            .ok_or_else(|| SchedulerError::NotSchedulable(format!("task {task_id} not found")))?;
        if task.status != TaskStatus::Review {
            return Ok(None);
        }
        let project = self.store.get_project(&task.project_id)?.ok_or_else(|| {
            SchedulerError::NotSchedulable(format!("project {} not found", task.project_id))
        })?;
        let project_root = PathBuf::from(&project.root_path).canonicalize()?;
        let required = configured_gate_names(&project_root)?;
        let gates = self
            .store
            .list_gates(task_id)?
            .into_iter()
            .filter(|gate| run_id.is_none_or(|id| gate.run_id.as_ref() == Some(id)))
            .collect::<Vec<_>>();
        if required.is_empty()
            || evaluate_required_gates(&gates, &required) != GateEligibility::Eligible
        {
            return Ok(None);
        }
        verify_recorded_gate_commands(&project_root, &gates, &required)?;
        if !task.write {
            return self.complete_read_review(task, run_id, &required).map(Some);
        }
        let mut worktree = self
            .store
            .list_worktrees(&task.project_id)?
            .into_iter()
            .find(|candidate| {
                candidate.task_id == *task_id
                    && run_id.is_none_or(|id| candidate.run_id.as_ref() == Some(id))
                    && matches!(
                        candidate.status,
                        WorktreeStatus::Detached | WorktreeStatus::Active
                    )
            })
            .ok_or_else(|| {
                SchedulerError::NotSchedulable("eligible task has no active worktree".into())
            })?;
        let base_revision = worktree.base_revision.clone().ok_or_else(|| {
            SchedulerError::NotSchedulable("worktree has no persisted base revision".into())
        })?;
        let persisted_scope = WorktreeHandle {
            task_id: task.id.to_string(),
            path: PathBuf::from(&worktree.path).canonicalize()?,
            base_revision,
            write_scope: WriteScope::new(
                worktree
                    .write_scope
                    .iter()
                    .map(RepoPath::new)
                    .collect::<Result<Vec<_>, _>>()?,
            )?,
        };
        let manager = WorktreeManager::open(
            &project_root,
            self.worktree_root.join(task.project_id.as_str()),
        )?;
        let workstream = task
            .workstream_id
            .as_ref()
            .map(|id| self.store.get_workstream(id))
            .transpose()?
            .flatten()
            .map_or_else(|| "default".into(), |item| branch_segment(&item.name));
        let finalized = manager.finalize(
            &persisted_scope,
            &workstream,
            &format!("ba({}): {}", task.id, task.title),
            true,
        )?;
        worktree.branch = Some(finalized.branch.clone());
        worktree.status = WorktreeStatus::Committed;
        worktree.updated_at = now();
        self.store.upsert_worktree(&worktree)?;
        self.store.transition_task(task_id, TaskStatus::Completed)?;
        let receipt_kind = write_receipt_kind(&finalized);
        let receipt = self.store.append_receipt(
            &task.project_id,
            Some(task.id),
            run_id.cloned(),
            receipt_kind,
            json!({
                "branch": finalized.branch,
                "commit": finalized.commit,
                "changedPaths": finalized.changed_paths,
                "requiredGates": required,
            }),
        )?;
        Ok(Some(FinalizationResult {
            worktree: Some(worktree),
            git: Some(finalized),
            receipt,
        }))
    }

    fn complete_read_review(
        &self,
        task: Task,
        run_id: Option<&EntityId>,
        required_gates: &[String],
    ) -> Result<FinalizationResult, SchedulerError> {
        let mut worktree = self
            .store
            .list_worktrees(&task.project_id)?
            .into_iter()
            .find(|candidate| {
                candidate.task_id == task.id
                    && run_id.is_some_and(|id| candidate.run_id.as_ref() == Some(id))
            });
        let commit = worktree
            .as_ref()
            .and_then(|item| item.base_revision.clone());
        if let Some(snapshot) = worktree.as_mut() {
            snapshot.status = WorktreeStatus::Committed;
            snapshot.updated_at = now();
            self.store.upsert_worktree(snapshot)?;
        }
        self.store
            .transition_task(&task.id, TaskStatus::Completed)?;
        let receipt = self.store.append_receipt(
            &task.project_id,
            Some(task.id),
            run_id.cloned(),
            "quality_gated_read_review",
            json!({ "requiredGates": required_gates, "commit": commit }),
        )?;
        Ok(FinalizationResult {
            worktree,
            git: None,
            receipt,
        })
    }
}

fn write_receipt_kind(finalized: &FinalizedWorktree) -> &'static str {
    if finalized.changed_paths.is_empty() {
        "quality_gated_noop_write"
    } else {
        "quality_gated_local_commit"
    }
}

fn verify_recorded_gate_commands(
    project_root: &std::path::Path,
    gates: &[Gate],
    required: &[String],
) -> Result<(), SchedulerError> {
    for name in required {
        let gate = gates
            .iter()
            .filter(|gate| gate.name == *name)
            .max_by(|left, right| left.created_at.cmp(&right.created_at))
            .ok_or_else(|| {
                SchedulerError::InvalidGate(format!("required gate is missing: {name}"))
            })?;
        let recorded = gate.command.as_deref().ok_or_else(|| {
            SchedulerError::InvalidGate(format!("gate command is missing: {name}"))
        })?;
        let recorded = serde_json::from_str::<RecordedGateCommand>(recorded)?;
        let configured = configured_gate(project_root, name)?;
        if recorded.program != configured.program || recorded.args != configured.args {
            return Err(SchedulerError::InvalidGate(format!(
                "recorded gate command does not match project configuration: {name}"
            )));
        }
    }
    Ok(())
}

fn branch_segment(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in value.trim().chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_') {
            result.extend(character.to_lowercase());
            separator = false;
        } else if !result.is_empty() && !separator {
            result.push('-');
            separator = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "default".into()
    } else {
        result
    }
}
