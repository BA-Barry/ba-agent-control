use std::collections::BTreeMap;

use ba_core::{
    EntityId, InactiveTask, RunStatus, Task, TaskDisposition, TaskDispositionAction, TaskStatus,
    now,
};
use ba_store::SqliteStore;

use crate::SchedulerError;

#[derive(Clone, Debug)]
pub struct TaskDispositionService {
    store: SqliteStore,
}

impl TaskDispositionService {
    #[must_use]
    pub const fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Return the project's default active register without changing the complete task graph.
    ///
    /// # Errors
    ///
    /// Returns an error when Task or disposition persistence cannot be read.
    pub fn active_tasks(&self, project_id: &EntityId) -> Result<Vec<Task>, SchedulerError> {
        let hidden = self.hidden_task_ids(project_id)?;
        Ok(self
            .store
            .list_tasks(project_id)?
            .into_iter()
            .filter(|task| !hidden.contains_key(&task.id))
            .collect())
    }

    /// Return inactive Tasks with the immutable decision that currently hides each one.
    ///
    /// # Errors
    ///
    /// Returns an error when a disposition references a missing Task or persistence fails.
    pub fn inactive_tasks(
        &self,
        project_id: &EntityId,
    ) -> Result<Vec<InactiveTask>, SchedulerError> {
        self.store
            .list_current_task_dispositions(project_id)?
            .into_iter()
            .filter(|record| record.action.hides_task())
            .map(|disposition| {
                let task = self.task(&disposition.task_id)?;
                Ok(InactiveTask { task, disposition })
            })
            .collect()
    }

    /// Read the append-only visibility-decision history for one Task.
    ///
    /// # Errors
    ///
    /// Returns an error when the Task is missing or persistence fails.
    pub fn history(&self, task_id: &EntityId) -> Result<Vec<TaskDisposition>, SchedulerError> {
        self.task(task_id)?;
        self.store
            .list_task_dispositions(task_id)
            .map_err(Into::into)
    }

    /// Hide an obsolete Task without changing its lifecycle status.
    ///
    /// # Errors
    ///
    /// Returns an error when the Task is active, already hidden, or still required by visible work.
    pub fn archive(
        &self,
        task_id: &EntityId,
        reason: &str,
    ) -> Result<TaskDisposition, SchedulerError> {
        self.record(task_id, TaskDispositionAction::Archived, reason, None)
    }

    /// Hide an obsolete Task and preserve a same-project replacement reference.
    ///
    /// # Errors
    ///
    /// Returns an error when either Task is invalid, active, hidden, or dependency-linked unsafely.
    pub fn supersede(
        &self,
        task_id: &EntityId,
        replacement_task_id: &EntityId,
        reason: &str,
    ) -> Result<TaskDisposition, SchedulerError> {
        self.record(
            task_id,
            TaskDispositionAction::Superseded,
            reason,
            Some(replacement_task_id),
        )
    }

    /// Restore a hidden Task to the default active register while retaining its earlier decisions.
    ///
    /// # Errors
    ///
    /// Returns an error when the Task is not currently archived or superseded.
    pub fn restore(
        &self,
        task_id: &EntityId,
        reason: &str,
    ) -> Result<TaskDisposition, SchedulerError> {
        self.record(task_id, TaskDispositionAction::Restored, reason, None)
    }

    /// Reject public lifecycle or dispatch operations against a hidden Task.
    ///
    /// # Errors
    ///
    /// Returns an error when the Task is hidden or persistence cannot be read.
    pub fn ensure_active(&self, task_id: &EntityId) -> Result<(), SchedulerError> {
        self.task(task_id)?;
        if let Some(current) = self.store.get_current_task_disposition(task_id)?
            && current.action.hides_task()
        {
            return Err(SchedulerError::TaskDispositionConflict(format!(
                "task {task_id} is {}; restore it before changing status or dispatching",
                current.action.as_str()
            )));
        }
        Ok(())
    }

    fn record(
        &self,
        task_id: &EntityId,
        action: TaskDispositionAction,
        reason: &str,
        replacement_task_id: Option<&EntityId>,
    ) -> Result<TaskDisposition, SchedulerError> {
        let task = self.task(task_id)?;
        let reason = required_reason(reason)?;
        let current = self.store.get_current_task_disposition(task_id)?;
        match action {
            TaskDispositionAction::Archived | TaskDispositionAction::Superseded => {
                if current
                    .as_ref()
                    .is_some_and(|item| item.action.hides_task())
                {
                    return Err(SchedulerError::TaskDispositionConflict(format!(
                        "task {task_id} is already hidden"
                    )));
                }
                self.ensure_disposable(&task)?;
            }
            TaskDispositionAction::Restored => {
                if !current
                    .as_ref()
                    .is_some_and(|item| item.action.hides_task())
                {
                    return Err(SchedulerError::TaskDispositionConflict(format!(
                        "task {task_id} is already active"
                    )));
                }
            }
        }
        let replacement_task_id = match action {
            TaskDispositionAction::Superseded => Some(self.validate_replacement(
                &task,
                replacement_task_id.ok_or_else(|| {
                    SchedulerError::InvalidTaskDisposition(
                        "supersede requires a replacement Task".into(),
                    )
                })?,
            )?),
            TaskDispositionAction::Archived | TaskDispositionAction::Restored => None,
        };
        let record = TaskDisposition {
            id: EntityId::new(),
            task_id: task.id,
            action,
            reason,
            replacement_task_id,
            previous_disposition_id: current.as_ref().map(|item| item.id.clone()),
            created_at: now(),
        };
        self.store
            .append_task_disposition(&record, current.as_ref().map(|item| &item.id))?;
        Ok(record)
    }

    fn ensure_disposable(&self, task: &Task) -> Result<(), SchedulerError> {
        if matches!(
            task.status,
            TaskStatus::Queued | TaskStatus::Running | TaskStatus::Review
        ) {
            return Err(SchedulerError::TaskDispositionConflict(format!(
                "task {} is {}; stop or finish it before hiding it",
                task.id, task.status
            )));
        }
        if self.store.list_runs(&task.id)?.iter().any(|run| {
            matches!(
                run.status,
                RunStatus::Queued
                    | RunStatus::Starting
                    | RunStatus::Running
                    | RunStatus::WaitingApproval
            )
        }) {
            return Err(SchedulerError::TaskDispositionConflict(format!(
                "task {} still has an active or queued Run",
                task.id
            )));
        }
        let hidden = self.hidden_task_ids(&task.project_id)?;
        let dependents = self
            .store
            .list_tasks(&task.project_id)?
            .into_iter()
            .filter(|candidate| {
                candidate.depends_on.contains(&task.id)
                    && !hidden.contains_key(&candidate.id)
                    && !matches!(
                        candidate.status,
                        TaskStatus::Completed | TaskStatus::Cancelled
                    )
            })
            .map(|candidate| candidate.id.to_string())
            .collect::<Vec<_>>();
        if !dependents.is_empty() {
            return Err(SchedulerError::TaskDispositionConflict(format!(
                "task {} is still required by visible unfinished tasks: {}",
                task.id,
                dependents.join(", ")
            )));
        }
        Ok(())
    }

    fn validate_replacement(
        &self,
        original: &Task,
        replacement_id: &EntityId,
    ) -> Result<EntityId, SchedulerError> {
        if replacement_id == &original.id {
            return Err(SchedulerError::InvalidTaskDisposition(
                "replacement Task must differ from the superseded Task".into(),
            ));
        }
        let replacement = self.task(replacement_id)?;
        if replacement.project_id != original.project_id {
            return Err(SchedulerError::InvalidTaskDisposition(
                "replacement Task belongs to a different project".into(),
            ));
        }
        if replacement.status == TaskStatus::Cancelled {
            return Err(SchedulerError::TaskDispositionConflict(
                "replacement Task is cancelled".into(),
            ));
        }
        if replacement.depends_on.contains(&original.id) {
            return Err(SchedulerError::TaskDispositionConflict(
                "replacement Task still depends on the Task being superseded".into(),
            ));
        }
        if self
            .store
            .get_current_task_disposition(replacement_id)?
            .is_some_and(|item| item.action.hides_task())
        {
            return Err(SchedulerError::TaskDispositionConflict(
                "replacement Task is not in the active register".into(),
            ));
        }
        Ok(replacement.id)
    }

    fn hidden_task_ids(
        &self,
        project_id: &EntityId,
    ) -> Result<BTreeMap<EntityId, TaskDisposition>, SchedulerError> {
        Ok(self
            .store
            .list_current_task_dispositions(project_id)?
            .into_iter()
            .filter(|record| record.action.hides_task())
            .map(|record| (record.task_id.clone(), record))
            .collect())
    }

    fn task(&self, id: &EntityId) -> Result<Task, SchedulerError> {
        self.store
            .get_task(id)?
            .ok_or_else(|| SchedulerError::TaskDispositionNotFound(id.to_string()))
    }
}

fn required_reason(value: &str) -> Result<String, SchedulerError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 2_000 {
        return Err(SchedulerError::InvalidTaskDisposition(
            "disposition reason must be between 1 and 2000 characters".into(),
        ));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use ba_core::{EntityId, Project, Task, TaskStatus, now};
    use ba_store::SqliteStore;

    use super::TaskDispositionService;

    fn task(project_id: &EntityId, title: &str, status: TaskStatus) -> Task {
        let stamp = now();
        Task {
            id: EntityId::new(),
            project_id: project_id.clone(),
            workstream_id: None,
            milestone_id: None,
            title: title.into(),
            description: String::new(),
            status,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            agent_profile_id: None,
            created_at: stamp,
            updated_at: stamp,
        }
    }

    #[test]
    fn dispositions_hide_without_rewriting_status_and_restore_with_history()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = SqliteStore::open(directory.path().join("state.sqlite3"))?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "disposition fixture".into(),
            root_path: directory.path().to_string_lossy().into_owned(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        let obsolete = task(&project.id, "obsolete", TaskStatus::Blocked);
        let replacement = task(&project.id, "replacement", TaskStatus::Draft);
        let mut dependent = task(&project.id, "dependent", TaskStatus::Ready);
        dependent.depends_on.push(obsolete.id.clone());
        for item in [&obsolete, &replacement, &dependent] {
            store.upsert_task(item)?;
        }
        let service = TaskDispositionService::new(store.clone());
        assert!(
            service
                .supersede(&obsolete.id, &replacement.id, "replacement is verified")
                .is_err()
        );
        service.archive(&dependent.id, "downstream work was abandoned")?;
        let superseded =
            service.supersede(&obsolete.id, &replacement.id, "replacement is verified")?;
        assert_eq!(superseded.replacement_task_id, Some(replacement.id.clone()));
        assert_eq!(
            store.get_task(&obsolete.id)?.map(|item| item.status),
            Some(TaskStatus::Blocked)
        );
        assert_eq!(service.active_tasks(&project.id)?.len(), 1);
        assert_eq!(service.inactive_tasks(&project.id)?.len(), 2);

        let restored = service.restore(&obsolete.id, "reopened for historical comparison")?;
        assert_eq!(restored.previous_disposition_id, Some(superseded.id));
        assert_eq!(service.history(&obsolete.id)?.len(), 2);
        assert_eq!(service.active_tasks(&project.id)?.len(), 2);
        Ok(())
    }
}
