use std::{collections::BTreeSet, path::PathBuf};

use ba_core::{
    Dispatch, EntityId, Run, RunStatus, Task, TaskStatus, Worktree, WorktreeStatus, now,
};
use ba_git::{RepoPath, WorktreeManager, WriteScope};
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Scheduler, SchedulerConfig, SchedulerError};

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
    config: SchedulerConfig,
}

impl DispatchCoordinator {
    #[must_use]
    pub fn new(store: SqliteStore, worktree_root: PathBuf) -> Self {
        Self {
            store,
            worktree_root,
            config: SchedulerConfig::default(),
        }
    }

    /// Override the default four-run concurrency limit.
    #[must_use]
    pub const fn with_config(mut self, config: SchedulerConfig) -> Self {
        self.config = config;
        self
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
        if let Some(profile_id) = agent_profile_id {
            if self.store.get_agent_profile(profile_id)?.is_none() {
                return Err(SchedulerError::NotSchedulable(format!(
                    "agent profile {profile_id} does not exist"
                )));
            }
        }
        let mut task = self
            .store
            .get_task(task_id)?
            .ok_or_else(|| SchedulerError::NotSchedulable(format!("task {task_id} not found")))?;
        if task.status == TaskStatus::Draft {
            task = self.store.transition_task(task_id, TaskStatus::Ready)?;
        }

        self.admit(&task)?;
        let stamp = now();
        let run = Run {
            id: EntityId::new(),
            task_id: task.id.clone(),
            agent_profile_id: agent_profile_id.cloned(),
            status: RunStatus::Queued,
            adapter: "codex".into(),
            runtime_run_id: None,
            runtime_thread_id: None,
            started_at: None,
            finished_at: None,
            created_at: stamp,
            updated_at: stamp,
        };
        let worktree = self.create_worktree(&task, &run.id)?;

        self.store.transition_task(task_id, TaskStatus::Queued)?;
        self.store.upsert_run(&run)?;
        let dispatch = Dispatch {
            id: EntityId::new(),
            task_id: task.id,
            run_id: run.id.clone(),
            agent_profile_id: agent_profile_id.cloned(),
            status: "queued".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        self.store.upsert_dispatch(&dispatch)?;

        let worktree = if let Some(mut worktree) = worktree {
            worktree.run_id = Some(run.id.clone());
            worktree.status = WorktreeStatus::Active;
            worktree.updated_at = now();
            self.store.upsert_worktree(&worktree)?;
            Some(worktree)
        } else {
            None
        };
        self.store.append_run_event(
            &run.id,
            "dispatch.queued",
            json!({
                "dispatch_id": dispatch.id,
                "worktree_id": worktree.as_ref().map(|item| &item.id)
            }),
        )?;
        Ok(DispatchBundle {
            dispatch,
            run,
            worktree,
        })
    }

    fn admit(&self, task: &Task) -> Result<(), SchedulerError> {
        let all_project_tasks = self.store.list_tasks(&task.project_id)?;
        let mut scheduler = Scheduler::new(self.config)?;
        let mut restored = BTreeSet::new();
        for status in [
            RunStatus::Queued,
            RunStatus::Starting,
            RunStatus::Running,
            RunStatus::WaitingApproval,
            RunStatus::Orphaned,
        ] {
            for active_run in self.store.list_runs_by_status(status)? {
                if !restored.insert(active_run.task_id.clone()) {
                    continue;
                }
                if let Some(active_task) = self.store.get_task(&active_run.task_id)? {
                    scheduler.restore(&active_task)?;
                }
            }
        }
        scheduler.admit(task, &all_project_tasks)?;
        Ok(())
    }

    fn create_worktree(
        &self,
        task: &Task,
        run_id: &EntityId,
    ) -> Result<Option<Worktree>, SchedulerError> {
        if !task.write {
            return Ok(None);
        }
        let project = self.store.get_project(&task.project_id)?.ok_or_else(|| {
            SchedulerError::NotSchedulable(format!("project {} not found", task.project_id))
        })?;
        let scope = WriteScope::new(
            task.path_scope
                .iter()
                .map(RepoPath::new)
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        let manager = WorktreeManager::open(
            &project.root_path,
            self.worktree_root.join(task.project_id.as_str()),
        )?;
        let attempt_name = format!(
            "{}-{}",
            task.id.as_str(),
            &run_id.as_str()[run_id.as_str().len() - 6..]
        );
        let handle = manager.create_detached(&attempt_name, scope)?;
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
}
