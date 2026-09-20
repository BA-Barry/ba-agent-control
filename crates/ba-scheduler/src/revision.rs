use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use ba_core::{EntityId, Task, TaskStatus};
use ba_git::WorktreeManager;
use ba_store::SqliteStore;

use crate::SchedulerError;

/// Resolve the most recent commit containing all completed dependency outputs.
/// Divergent outputs need an explicit integration decision, never an implicit merge.
pub(crate) fn dependency_revision(
    store: &SqliteStore,
    task: &Task,
    manager: &WorktreeManager,
) -> Result<Option<String>, SchedulerError> {
    let receipts = store.list_receipts(&task.project_id)?;
    let mut pending = task.depends_on.clone();
    let mut visited = BTreeSet::from([task.id.clone()]);
    let mut commits = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        let dependency = store
            .get_task(&id)?
            .ok_or_else(|| SchedulerError::NotSchedulable(format!("dependency {id} is missing")))?;
        if dependency.project_id != task.project_id || dependency.status != TaskStatus::Completed {
            return Err(SchedulerError::NotSchedulable(format!(
                "dependency {id} must be completed in the same project"
            )));
        }
        let receipt = receipts.iter().rev().find(|receipt| {
            receipt.task_id.as_ref() == Some(&id)
                && matches!(
                    receipt.kind.as_str(),
                    "quality_gated_local_commit"
                        | "quality_gated_noop_write"
                        | "quality_gated_read_review"
                )
        });
        if let Some(commit) = receipt
            .and_then(|receipt| receipt.payload.get("commit"))
            .and_then(serde_json::Value::as_str)
        {
            // Receipts must select an immutable object, not a branch which may move.
            if !matches!(commit.len(), 40 | 64)
                || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(SchedulerError::NotSchedulable(format!(
                    "dependency {id} has an invalid recorded commit"
                )));
            }
            commits.insert(manager.resolve_commit(commit)?);
        } else if dependency.write {
            return Err(SchedulerError::NotSchedulable(format!(
                "completed write dependency {id} has no finalized commit receipt"
            )));
        }
        pending.extend(dependency.depends_on);
    }
    for candidate in &commits {
        let mut includes_all = true;
        for other in &commits {
            if !manager.is_ancestor(other, candidate)? {
                includes_all = false;
                break;
            }
        }
        if includes_all {
            return Ok(Some(candidate.clone()));
        }
    }
    if commits.is_empty() {
        Ok(None)
    } else {
        // An already-integrated user checkout can resolve divergent inputs.
        // This query never merges or moves a ref itself.
        let head = manager.resolve_commit("HEAD")?;
        let mut integrated = true;
        for commit in &commits {
            integrated &= manager.is_ancestor(commit, &head)?;
        }
        if integrated {
            return Ok(Some(head));
        }
        Err(SchedulerError::NotSchedulable(format!(
            "dependency commits diverge; explicitly integrate these revisions before dispatch: {}",
            commits.into_iter().collect::<Vec<_>>().join(", ")
        )))
    }
}

/// Select the originating Run's checkout for quality commands on writers or readers.
///
/// # Errors
/// Returns an error for an absent/foreign Run, a missing required checkout, or an
/// unavailable path. Never silently substitutes the project root for a missing snapshot.
pub fn task_execution_directory(
    store: &SqliteStore,
    task: &Task,
    run_id: Option<&EntityId>,
    project_root: &Path,
) -> Result<PathBuf, SchedulerError> {
    let worktrees = store.list_worktrees(&task.project_id)?;
    if let Some(run_id) = run_id {
        let run = store
            .get_run(run_id)?
            .ok_or_else(|| SchedulerError::NotActive(run_id.to_string()))?;
        if run.task_id != task.id {
            return Err(SchedulerError::NotActive(
                "run belongs to a different task".into(),
            ));
        }
        if let Some(worktree) = worktrees.iter().find(|worktree| {
            worktree.task_id == task.id && worktree.run_id.as_ref() == Some(run_id)
        }) {
            return Ok(PathBuf::from(&worktree.path).canonicalize()?);
        }
    }
    if task.write || worktrees.iter().any(|worktree| worktree.task_id == task.id) {
        return Err(SchedulerError::NotSchedulable(
            "quality commands require the originating run and its worktree".into(),
        ));
    }
    Ok(project_root.canonicalize()?)
}
