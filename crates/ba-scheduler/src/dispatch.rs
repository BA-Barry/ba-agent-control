use std::path::PathBuf;

use ba_core::{
    Dispatch, EntityId, Run, RunStatus, Task, TaskStatus, Worktree, WorktreeStatus, now,
};
use ba_git::{RepoPath, WorktreeManager, WriteScope};
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{SchedulerError, TaskDispositionService, queue_blockers_for};

/// Durable result of admitting and queueing one task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchBundle {
    pub dispatch: Dispatch,
    pub run: Run,
    pub worktree: Option<Worktree>,
}

/// Policy boundary for dependency/concurrency admission and write-task isolation.
#[derive(Clone, Debug)]
pub struct DispatchCoordinator {
    store: SqliteStore,
    worktree_root: PathBuf,
}

impl DispatchCoordinator {
    #[must_use]
    pub fn new(store: SqliteStore, worktree_root: PathBuf) -> Self {
        Self {
            store,
            worktree_root,
        }
    }

    /// Admit a task, create its detached worktree when needed, and persist a queued run.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] when dependencies, capacity, write scopes, Git,
    /// entity references, or durable persistence prevent dispatch.
    pub fn enqueue(
        &self,
        task_id: &EntityId,
        agent_profile_id: Option<&EntityId>,
    ) -> Result<DispatchBundle, SchedulerError> {
        self.enqueue_with_prompt(task_id, agent_profile_id, None)
    }

    /// Persist a task in the durable global queue with an optional prompt override.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid task/profile, duplicate active task, unsafe
    /// write scope, dependency graph defect, or failed atomic persistence.
    pub fn enqueue_with_prompt(
        &self,
        task_id: &EntityId,
        agent_profile_id: Option<&EntityId>,
        prompt: Option<String>,
    ) -> Result<DispatchBundle, SchedulerError> {
        TaskDispositionService::new(self.store.clone()).ensure_active(task_id)?;
        let mut task = self
            .store
            .get_task(task_id)?
            .ok_or_else(|| SchedulerError::NotSchedulable(format!("task {task_id} not found")))?;
        let agent_profile_id = agent_profile_id
            .cloned()
            .or_else(|| task.agent_profile_id.clone());
        if let Some(profile_id) = &agent_profile_id
            && self.store.get_agent_profile(profile_id)?.is_none()
        {
            return Err(SchedulerError::NotSchedulable(format!(
                "agent profile {profile_id} does not exist"
            )));
        }
        if task.status == TaskStatus::Draft {
            task = self.store.transition_task(task_id, TaskStatus::Ready)?;
        }

        let project_tasks = self.store.list_tasks(&task.project_id)?;
        let blockers = queue_blockers_for(&task, &project_tasks);
        if !blockers.is_empty() {
            return Err(SchedulerError::NotSchedulable(serde_json::to_string(
                &blockers,
            )?));
        }
        for status in [
            RunStatus::Queued,
            RunStatus::Starting,
            RunStatus::Running,
            RunStatus::WaitingApproval,
            RunStatus::Orphaned,
        ] {
            if self
                .store
                .list_runs_by_status(status)?
                .iter()
                .any(|run| run.task_id == task.id)
            {
                return Err(SchedulerError::AlreadyActive(task.id.to_string()));
            }
        }
        let stamp = now();
        let run = Run {
            id: EntityId::new(),
            task_id: task.id.clone(),
            agent_profile_id: agent_profile_id.clone(),
            status: RunStatus::Queued,
            adapter: "codex".into(),
            runtime_run_id: None,
            runtime_thread_id: None,
            started_at: None,
            finished_at: None,
            created_at: stamp,
            updated_at: stamp,
        };
        let mut dispatch = Dispatch {
            id: EntityId::new(),
            task_id: task.id,
            run_id: run.id.clone(),
            agent_profile_id,
            status: "queued".into(),
            queue_position: 0,
            prompt: prompt.filter(|value| !value.trim().is_empty()),
            created_at: stamp,
            updated_at: stamp,
        };
        self.store.queue_dispatch(task_id, &run, &mut dispatch)?;
        self.store.append_run_event(
            &run.id,
            "dispatch.queued",
            json!({
                "dispatch_id": dispatch.id,
                "queue_position": dispatch.queue_position
            }),
        )?;
        Ok(DispatchBundle {
            dispatch,
            run,
            worktree: None,
        })
    }

    /// Create a detached write worktree without activating it.
    ///
    /// # Errors
    ///
    /// Returns an error when project lookup, scope validation, or Git worktree
    /// creation fails.
    pub fn create_worktree(
        &self,
        task: &Task,
        run_id: &EntityId,
    ) -> Result<Option<Worktree>, SchedulerError> {
        if !task.write && task.depends_on.is_empty() {
            return Ok(None);
        }
        let project = self.store.get_project(&task.project_id)?.ok_or_else(|| {
            SchedulerError::NotSchedulable(format!("project {} not found", task.project_id))
        })?;
        let scope = if task.write {
            WriteScope::new(
                task.path_scope
                    .iter()
                    .map(RepoPath::new)
                    .collect::<Result<Vec<_>, _>>()?,
            )?
        } else {
            WriteScope::read_only()
        };
        let manager = WorktreeManager::open(
            &project.root_path,
            self.worktree_root.join(task.project_id.as_str()),
        )?;
        let revision = crate::revision::dependency_revision(&self.store, task, &manager)?;
        let attempt_name = format!(
            "{}-{}",
            task.id.as_str(),
            &run_id.as_str()[run_id.as_str().len() - 6..]
        );
        let handle = manager.create_detached_at(
            &attempt_name,
            scope,
            revision.as_deref().unwrap_or("HEAD"),
        )?;
        let stamp = now();
        let worktree = Worktree {
            id: EntityId::new(),
            project_id: task.project_id.clone(),
            task_id: task.id.clone(),
            run_id: None,
            path: handle.path.to_string_lossy().into_owned(),
            base_revision: Some(handle.base_revision),
            write_scope: handle
                .write_scope
                .paths()
                .iter()
                .map(|path| path.as_path().to_string_lossy().into_owned())
                .collect(),
            branch: None,
            status: WorktreeStatus::Detached,
            created_at: stamp,
            updated_at: stamp,
        };
        self.store.upsert_worktree(&worktree)?;
        Ok(Some(worktree))
    }

    /// Create and durably attach a write worktree to a claimed run.
    ///
    /// # Errors
    ///
    /// Returns an error when worktree creation or persistence fails.
    pub fn activate_worktree(
        &self,
        task: &Task,
        run_id: &EntityId,
    ) -> Result<Option<Worktree>, SchedulerError> {
        let worktree = self.create_worktree(task, run_id)?;
        if let Some(mut worktree) = worktree {
            worktree.run_id = Some(run_id.clone());
            worktree.status = WorktreeStatus::Active;
            worktree.updated_at = now();
            self.store.upsert_worktree(&worktree)?;
            Ok(Some(worktree))
        } else {
            Ok(None)
        }
    }
}
