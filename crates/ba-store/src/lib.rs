#![allow(
    clippy::missing_errors_doc,
    reason = "all persistence methods uniformly surface the documented StoreError"
)]

//! SQLite persistence for BA Agent.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use ba_core::{
    AgentProfile, Approval, Artifact, Dispatch, EntityId, Gate, Milestone, Project, Receipt,
    ReceiptChainError, ReceiptDraft, Run, RunEvent, RunStatus, Task, TaskStatus, Timestamp,
    Workstream, Worktree, compute_receipt_hash, now,
};
use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, Params, Transaction, backup::Backup, params};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

const CURRENT_SCHEMA_VERSION: u32 = 1;
const MIGRATION_0001: &str = include_str!("../migrations/0001_initial.sql");

/// Thread-safe handle to the process-wide SQLite database.
#[derive(Clone, Debug)]
pub struct SqliteStore {
    connection: Arc<Mutex<Connection>>,
    path: Arc<PathBuf>,
    last_backup: Arc<Option<PathBuf>>,
}

#[derive(Clone, Debug)]
pub struct TerminalRunUpdate {
    pub run: Run,
    pub task: Task,
    pub dispatches: Vec<Dispatch>,
    pub event: RunEvent,
}

impl SqliteStore {
    /// Open a database, verify it, back it up if a migration is pending, and migrate atomically.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let existed = path.exists() && fs::metadata(&path)?.len() > 0;
        let mut connection = Connection::open(&path)?;
        configure(&connection)?;
        integrity_check(&connection)?;

        let version = schema_version(&connection)?;
        if version > CURRENT_SCHEMA_VERSION {
            return Err(StoreError::SchemaTooNew {
                found: version,
                supported: CURRENT_SCHEMA_VERSION,
            });
        }

        let backup = if existed && version < CURRENT_SCHEMA_VERSION {
            Some(create_backup(&connection, &path, version)?)
        } else {
            None
        };

        migrate(&mut connection, version)?;
        integrity_check(&connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path: Arc::new(path),
            last_backup: Arc::new(backup),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }

    #[must_use]
    pub fn last_backup(&self) -> Option<&Path> {
        self.last_backup.as_deref()
    }

    pub fn integrity_check(&self) -> Result<(), StoreError> {
        integrity_check(&self.connection.lock())
    }

    pub fn schema_version(&self) -> Result<u32, StoreError> {
        schema_version(&self.connection.lock())
    }

    pub fn upsert_project(&self, project: &Project) -> Result<(), StoreError> {
        let data = json(project)?;
        self.connection.lock().execute(
            "INSERT INTO projects(id, root_path, updated_at, data) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(id) DO UPDATE SET root_path=excluded.root_path, updated_at=excluded.updated_at, data=excluded.data",
            params![project.id.as_str(), project.root_path, timestamp(project.updated_at), data],
        )?;
        Ok(())
    }

    pub fn get_project(&self, id: &EntityId) -> Result<Option<Project>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM projects WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM projects ORDER BY updated_at DESC",
            [],
        )
    }

    pub fn delete_project(&self, id: &EntityId) -> Result<bool, StoreError> {
        delete_by_id(&self.connection.lock(), "projects", id)
    }

    pub fn upsert_workstream(&self, item: &Workstream) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO workstreams(id, project_id, status, updated_at, data) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id,status=excluded.status,updated_at=excluded.updated_at,data=excluded.data",
            params![item.id.as_str(), item.project_id.as_str(), item.status, timestamp(item.updated_at), data],
        )?;
        Ok(())
    }

    pub fn get_workstream(&self, id: &EntityId) -> Result<Option<Workstream>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM workstreams WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn list_workstreams(&self, project_id: &EntityId) -> Result<Vec<Workstream>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM workstreams WHERE project_id=?1 ORDER BY updated_at DESC",
            params![project_id.as_str()],
        )
    }

    pub fn delete_workstream(&self, id: &EntityId) -> Result<bool, StoreError> {
        delete_by_id(&self.connection.lock(), "workstreams", id)
    }

    pub fn upsert_milestone(&self, item: &Milestone) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO milestones(id,project_id,workstream_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id,workstream_id=excluded.workstream_id,status=excluded.status,updated_at=excluded.updated_at,data=excluded.data",
            params![item.id.as_str(), item.project_id.as_str(), optional_id(item.workstream_id.as_ref()), item.status, timestamp(item.updated_at), data],
        )?;
        Ok(())
    }

    /// Insert a milestone and atomically make it the sole active milestone for its project.
    pub fn create_milestone(&self, item: &Milestone) -> Result<(), StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        if !matches!(
            item.status.as_str(),
            "planned" | "active" | "completed" | "cancelled"
        ) {
            return Err(StoreError::InvalidEntityLink(
                "milestone status must be planned, active, completed, or cancelled",
            ));
        }
        if let Some(workstream_id) = &item.workstream_id {
            let workstream: Workstream = required_json(
                &transaction,
                "SELECT data FROM workstreams WHERE id=?1",
                params![workstream_id.as_str()],
                "workstream",
                workstream_id,
            )?;
            if workstream.project_id != item.project_id {
                return Err(StoreError::InvalidEntityLink(
                    "milestone and workstream must belong to the same project",
                ));
            }
        }
        if item.status == "active" {
            let mut statement = transaction
                .prepare("SELECT data FROM milestones WHERE project_id=?1 AND status='active'")?;
            let existing = statement
                .query_map(params![item.project_id.as_str()], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            for data in existing {
                let mut milestone: Milestone = serde_json::from_str(&data)?;
                milestone.status = "planned".into();
                milestone.updated_at = now();
                transaction.execute(
                    "UPDATE milestones SET status='planned',updated_at=?1,data=?2 WHERE id=?3",
                    params![
                        timestamp(milestone.updated_at),
                        json(&milestone)?,
                        milestone.id.as_str()
                    ],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO milestones(id,project_id,workstream_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                item.id.as_str(),
                item.project_id.as_str(),
                optional_id(item.workstream_id.as_ref()),
                item.status,
                timestamp(item.updated_at),
                json(item)?
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get_milestone(&self, id: &EntityId) -> Result<Option<Milestone>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM milestones WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn list_milestones(&self, project_id: &EntityId) -> Result<Vec<Milestone>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM milestones WHERE project_id=?1 ORDER BY updated_at DESC",
            params![project_id.as_str()],
        )
    }

    pub fn delete_milestone(&self, id: &EntityId) -> Result<bool, StoreError> {
        delete_by_id(&self.connection.lock(), "milestones", id)
    }

    pub fn upsert_task(&self, item: &Task) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO tasks(id,project_id,workstream_id,milestone_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5,?6,?7) \
             ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id,workstream_id=excluded.workstream_id,milestone_id=excluded.milestone_id,status=excluded.status,updated_at=excluded.updated_at,data=excluded.data",
            params![item.id.as_str(), item.project_id.as_str(), optional_id(item.workstream_id.as_ref()), optional_id(item.milestone_id.as_ref()), item.status.as_str(), timestamp(item.updated_at), data],
        )?;
        Ok(())
    }

    /// Create a new workstream and its first task in one transaction.
    ///
    /// This is intentionally insert-only: a generated identifier collision or a
    /// task constraint failure rolls the workstream insertion back instead of
    /// leaving an orphaned navigation record.
    pub fn create_workstream_and_task(
        &self,
        workstream: &Workstream,
        task: &Task,
    ) -> Result<(), StoreError> {
        if workstream.project_id != task.project_id
            || task.workstream_id.as_ref() != Some(&workstream.id)
        {
            return Err(StoreError::InvalidEntityLink(
                "workstream and task must belong to the same project and reference each other",
            ));
        }
        let workstream_data = json(workstream)?;
        let task_data = json(task)?;
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO workstreams(id,project_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5)",
            params![
                workstream.id.as_str(),
                workstream.project_id.as_str(),
                workstream.status,
                timestamp(workstream.updated_at),
                workstream_data
            ],
        )?;
        transaction.execute(
            "INSERT INTO tasks(id,project_id,workstream_id,milestone_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                task.id.as_str(),
                task.project_id.as_str(),
                optional_id(task.workstream_id.as_ref()),
                optional_id(task.milestone_id.as_ref()),
                task.status.as_str(),
                timestamp(task.updated_at),
                task_data
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get_task(&self, id: &EntityId) -> Result<Option<Task>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM tasks WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn list_tasks(&self, project_id: &EntityId) -> Result<Vec<Task>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM tasks WHERE project_id=?1 ORDER BY updated_at DESC",
            params![project_id.as_str()],
        )
    }

    pub fn list_tasks_by_status(
        &self,
        project_id: &EntityId,
        status: TaskStatus,
    ) -> Result<Vec<Task>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM tasks WHERE project_id=?1 AND status=?2 ORDER BY updated_at DESC",
            params![project_id.as_str(), status.as_str()],
        )
    }

    pub fn delete_task(&self, id: &EntityId) -> Result<bool, StoreError> {
        delete_by_id(&self.connection.lock(), "tasks", id)
    }

    pub fn transition_task(&self, id: &EntityId, next: TaskStatus) -> Result<Task, StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let mut task: Task = required_json(
            &transaction,
            "SELECT data FROM tasks WHERE id=?1",
            params![id.as_str()],
            "task",
            id,
        )?;
        task.status.ensure_transition(next)?;
        task.status = next;
        task.updated_at = now();
        let data = json(&task)?;
        transaction.execute(
            "UPDATE tasks SET status=?1,updated_at=?2,data=?3 WHERE id=?4",
            params![next.as_str(), timestamp(task.updated_at), data, id.as_str()],
        )?;
        transaction.commit()?;
        Ok(task)
    }

    pub fn upsert_agent_profile(&self, item: &AgentProfile) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO agent_profiles(id,role,runtime,updated_at,data) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(id) DO UPDATE SET role=excluded.role,runtime=excluded.runtime,updated_at=excluded.updated_at,data=excluded.data",
            params![item.id.as_str(), item.role, item.runtime, timestamp(item.updated_at), data],
        )?;
        Ok(())
    }

    pub fn get_agent_profile(&self, id: &EntityId) -> Result<Option<AgentProfile>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM agent_profiles WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn list_agent_profiles(&self) -> Result<Vec<AgentProfile>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM agent_profiles ORDER BY updated_at DESC",
            [],
        )
    }

    pub fn delete_agent_profile(&self, id: &EntityId) -> Result<bool, StoreError> {
        delete_by_id(&self.connection.lock(), "agent_profiles", id)
    }

    pub fn upsert_run(&self, item: &Run) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO runs(id,task_id,agent_profile_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(id) DO UPDATE SET task_id=excluded.task_id,agent_profile_id=excluded.agent_profile_id,status=excluded.status,updated_at=excluded.updated_at,data=excluded.data",
            params![item.id.as_str(), item.task_id.as_str(), optional_id(item.agent_profile_id.as_ref()), item.status.as_str(), timestamp(item.updated_at), data],
        )?;
        Ok(())
    }

    pub fn get_run(&self, id: &EntityId) -> Result<Option<Run>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM runs WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn list_runs(&self, task_id: &EntityId) -> Result<Vec<Run>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM runs WHERE task_id=?1 ORDER BY updated_at DESC",
            params![task_id.as_str()],
        )
    }

    pub fn list_runs_by_status(&self, status: RunStatus) -> Result<Vec<Run>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM runs WHERE status=?1 ORDER BY updated_at DESC",
            params![status.as_str()],
        )
    }

    pub fn list_project_runs(&self, project_id: &EntityId) -> Result<Vec<Run>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT runs.data FROM runs JOIN tasks ON tasks.id=runs.task_id WHERE tasks.project_id=?1 ORDER BY runs.updated_at DESC",
            params![project_id.as_str()],
        )
    }

    pub fn latest_run_event_sequence(&self, run_id: &EntityId) -> Result<u64, StoreError> {
        Ok(self.connection.lock().query_row(
            "SELECT COALESCE(MAX(sequence),0) FROM run_events WHERE run_id=?1",
            params![run_id.as_str()],
            |row| row.get(0),
        )?)
    }

    pub fn transition_run(&self, id: &EntityId, next: RunStatus) -> Result<Run, StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let mut run: Run = required_json(
            &transaction,
            "SELECT data FROM runs WHERE id=?1",
            params![id.as_str()],
            "run",
            id,
        )?;
        run.status.ensure_transition(next)?;
        run.status = next;
        run.updated_at = now();
        if next == RunStatus::Running && run.started_at.is_none() {
            run.started_at = Some(run.updated_at);
        }
        if matches!(
            next,
            RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
        ) {
            run.finished_at = Some(run.updated_at);
        }
        let data = json(&run)?;
        transaction.execute(
            "UPDATE runs SET status=?1,updated_at=?2,data=?3 WHERE id=?4",
            params![next.as_str(), timestamp(run.updated_at), data, id.as_str()],
        )?;
        transaction.commit()?;
        Ok(run)
    }

    /// Atomically finish a run and synchronize its task, dispatches, and terminal event.
    pub fn finish_run(
        &self,
        id: &EntityId,
        next: RunStatus,
        event_kind: impl Into<String>,
        event_payload: Value,
    ) -> Result<TerminalRunUpdate, StoreError> {
        let (task_status, dispatch_status) = match next {
            RunStatus::Succeeded => (TaskStatus::Review, "succeeded"),
            RunStatus::Failed => (TaskStatus::Failed, "failed"),
            RunStatus::Cancelled => (TaskStatus::Cancelled, "cancelled"),
            _ => {
                return Err(StoreError::InvalidTerminalRunStatus(next.to_string()));
            }
        };
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let mut run: Run = required_json(
            &transaction,
            "SELECT data FROM runs WHERE id=?1",
            params![id.as_str()],
            "run",
            id,
        )?;
        run.status.ensure_transition(next)?;
        let mut task: Task = required_json(
            &transaction,
            "SELECT data FROM tasks WHERE id=?1",
            params![run.task_id.as_str()],
            "task",
            &run.task_id,
        )?;
        task.status.ensure_transition(task_status)?;
        let stamp = now();
        run.status = next;
        run.updated_at = stamp;
        run.finished_at = Some(stamp);
        task.status = task_status;
        task.updated_at = stamp;
        transaction.execute(
            "UPDATE runs SET status=?1,updated_at=?2,data=?3 WHERE id=?4",
            params![next.as_str(), timestamp(stamp), json(&run)?, id.as_str()],
        )?;
        transaction.execute(
            "UPDATE tasks SET status=?1,updated_at=?2,data=?3 WHERE id=?4",
            params![
                task_status.as_str(),
                timestamp(stamp),
                json(&task)?,
                task.id.as_str()
            ],
        )?;

        let mut dispatches = {
            let mut statement = transaction
                .prepare("SELECT data FROM dispatches WHERE run_id=?1 ORDER BY updated_at DESC")?;
            let rows = statement.query_map(params![id.as_str()], |row| row.get::<_, String>(0))?;
            let mut values = Vec::new();
            for row in rows {
                values.push(serde_json::from_str::<Dispatch>(&row?)?);
            }
            values
        };
        for dispatch in &mut dispatches {
            dispatch.status = dispatch_status.into();
            dispatch.updated_at = stamp;
            transaction.execute(
                "UPDATE dispatches SET status=?1,updated_at=?2,data=?3 WHERE id=?4",
                params![
                    dispatch_status,
                    timestamp(stamp),
                    json(dispatch)?,
                    dispatch.id.as_str()
                ],
            )?;
        }
        let sequence: u64 = transaction.query_row(
            "SELECT COALESCE(MAX(sequence),0)+1 FROM run_events WHERE run_id=?1",
            params![id.as_str()],
            |row| row.get(0),
        )?;
        let event = RunEvent {
            run_id: id.clone(),
            sequence,
            kind: event_kind.into(),
            payload: event_payload,
            created_at: stamp,
        };
        transaction.execute(
            "INSERT INTO run_events(run_id,sequence,kind,payload,created_at) VALUES (?1,?2,?3,?4,?5)",
            params![event.run_id.as_str(), event.sequence, event.kind, json(&event.payload)?, timestamp(event.created_at)],
        )?;
        transaction.commit()?;
        Ok(TerminalRunUpdate {
            run,
            task,
            dispatches,
            event,
        })
    }

    pub fn upsert_dispatch(&self, item: &Dispatch) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO dispatches(id,task_id,run_id,agent_profile_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5,?6,?7) \
             ON CONFLICT(id) DO UPDATE SET status=excluded.status,updated_at=excluded.updated_at,data=excluded.data",
            params![item.id.as_str(),item.task_id.as_str(),item.run_id.as_str(),optional_id(item.agent_profile_id.as_ref()),item.status,timestamp(item.updated_at),data],
        )?;
        Ok(())
    }

    pub fn list_dispatches(&self, run_id: &EntityId) -> Result<Vec<Dispatch>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM dispatches WHERE run_id=?1 ORDER BY updated_at DESC",
            params![run_id.as_str()],
        )
    }

    /// Atomically update every dispatch attached to one run.
    pub fn update_dispatch_status(
        &self,
        run_id: &EntityId,
        status: &str,
    ) -> Result<Vec<Dispatch>, StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let mut dispatches = {
            let mut statement = transaction
                .prepare("SELECT data FROM dispatches WHERE run_id=?1 ORDER BY updated_at DESC")?;
            let rows =
                statement.query_map(params![run_id.as_str()], |row| row.get::<_, String>(0))?;
            let mut values = Vec::new();
            for row in rows {
                values.push(serde_json::from_str::<Dispatch>(&row?)?);
            }
            values
        };
        let stamp = now();
        for dispatch in &mut dispatches {
            dispatch.status = status.into();
            dispatch.updated_at = stamp;
            transaction.execute(
                "UPDATE dispatches SET status=?1,updated_at=?2,data=?3 WHERE id=?4",
                params![
                    status,
                    timestamp(stamp),
                    json(dispatch)?,
                    dispatch.id.as_str()
                ],
            )?;
        }
        transaction.commit()?;
        Ok(dispatches)
    }

    pub fn get_dispatch(&self, id: &EntityId) -> Result<Option<Dispatch>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM dispatches WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn upsert_approval(&self, item: &Approval) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO approvals(id,run_id,status,requested_at,data) VALUES (?1,?2,?3,?4,?5) \
             ON CONFLICT(id) DO UPDATE SET status=excluded.status,data=excluded.data",
            params![
                item.id.as_str(),
                item.run_id.as_str(),
                item.status.as_str(),
                timestamp(item.requested_at),
                data
            ],
        )?;
        Ok(())
    }

    pub fn list_approvals(&self, run_id: &EntityId) -> Result<Vec<Approval>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM approvals WHERE run_id=?1 ORDER BY requested_at DESC",
            params![run_id.as_str()],
        )
    }

    pub fn get_approval(&self, id: &EntityId) -> Result<Option<Approval>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM approvals WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn list_pending_approvals(&self) -> Result<Vec<Approval>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM approvals WHERE status='pending' ORDER BY requested_at",
            [],
        )
    }

    pub fn upsert_gate(&self, item: &Gate) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO gates(id,task_id,run_id,status,created_at,data) VALUES (?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(id) DO UPDATE SET status=excluded.status,data=excluded.data",
            params![item.id.as_str(),item.task_id.as_str(),optional_id(item.run_id.as_ref()),item.status.as_str(),timestamp(item.created_at),data],
        )?;
        Ok(())
    }

    pub fn list_gates(&self, task_id: &EntityId) -> Result<Vec<Gate>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM gates WHERE task_id=?1 ORDER BY created_at DESC",
            params![task_id.as_str()],
        )
    }

    pub fn get_gate(&self, id: &EntityId) -> Result<Option<Gate>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM gates WHERE id=?1",
            params![id.as_str()],
        )
    }

    /// Append a receipt while holding a transaction, linking it to the current project tail.
    pub fn append_receipt(
        &self,
        project_id: &EntityId,
        task_id: Option<EntityId>,
        run_id: Option<EntityId>,
        kind: impl Into<String>,
        payload: Value,
    ) -> Result<Receipt, StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let tail = transaction
            .query_row(
                "SELECT chain_sequence,hash FROM receipts WHERE project_id=?1 ORDER BY chain_sequence DESC LIMIT 1",
                params![project_id.as_str()],
                |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let (sequence, previous_hash) =
            tail.map_or((1, None), |(sequence, hash)| (sequence + 1, Some(hash)));
        let mut receipt = Receipt {
            id: EntityId::new(),
            project_id: project_id.clone(),
            task_id,
            run_id,
            kind: kind.into(),
            payload,
            previous_hash,
            hash: String::new(),
            created_at: now(),
        };
        receipt.hash = compute_receipt_hash(&ReceiptDraft::from(&receipt))?;
        insert_receipt(&transaction, &receipt, sequence)?;
        transaction.commit()?;
        Ok(receipt)
    }

    /// Persist an artifact and its permanent receipt in one SQLite transaction.
    pub fn record_artifact_with_receipt(
        &self,
        artifact: &Artifact,
        receipt_kind: impl Into<String>,
        payload: Value,
    ) -> Result<Receipt, StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let artifact_data = json(artifact)?;
        transaction.execute(
            "INSERT INTO artifacts(id,project_id,task_id,run_id,created_at,data) VALUES (?1,?2,?3,?4,?5,?6)",
            params![artifact.id.as_str(),artifact.project_id.as_str(),optional_id(artifact.task_id.as_ref()),optional_id(artifact.run_id.as_ref()),timestamp(artifact.created_at),artifact_data],
        )?;
        let tail = transaction
            .query_row(
                "SELECT chain_sequence,hash FROM receipts WHERE project_id=?1 ORDER BY chain_sequence DESC LIMIT 1",
                params![artifact.project_id.as_str()],
                |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let (sequence, previous_hash) =
            tail.map_or((1, None), |(sequence, hash)| (sequence + 1, Some(hash)));
        let mut receipt = Receipt {
            id: EntityId::new(),
            project_id: artifact.project_id.clone(),
            task_id: artifact.task_id.clone(),
            run_id: artifact.run_id.clone(),
            kind: receipt_kind.into(),
            payload,
            previous_hash,
            hash: String::new(),
            created_at: now(),
        };
        receipt.hash = compute_receipt_hash(&ReceiptDraft::from(&receipt))?;
        insert_receipt(&transaction, &receipt, sequence)?;
        transaction.commit()?;
        Ok(receipt)
    }

    pub fn list_receipts(&self, project_id: &EntityId) -> Result<Vec<Receipt>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM receipts WHERE project_id=?1 ORDER BY chain_sequence",
            params![project_id.as_str()],
        )
    }

    pub fn get_receipt(&self, id: &EntityId) -> Result<Option<Receipt>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM receipts WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn verify_receipt_chain(&self, project_id: &EntityId) -> Result<(), StoreError> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            "SELECT chain_sequence,id,project_id,task_id,run_id,hash,previous_hash,created_at,data \
             FROM receipts WHERE project_id=?1 ORDER BY chain_sequence",
        )?;
        let rows = statement.query_map(params![project_id.as_str()], |row| {
            Ok(StoredReceipt {
                sequence: row.get(0)?,
                id: row.get(1)?,
                project_id: row.get(2)?,
                task_id: row.get(3)?,
                run_id: row.get(4)?,
                hash: row.get(5)?,
                previous_hash: row.get(6)?,
                created_at: row.get(7)?,
                data: row.get(8)?,
            })
        })?;

        let mut expected_previous: Option<String> = None;
        for (expected_sequence, row) in (1_u64..).zip(rows) {
            let row = row?;
            let receipt: Receipt = serde_json::from_str(&row.data)?;
            if row.sequence != expected_sequence {
                return Err(ReceiptChainError::Sequence {
                    id: receipt.id,
                    expected: expected_sequence,
                    actual: row.sequence,
                }
                .into());
            }
            ensure_receipt_field(&receipt, receipt.id.as_str() == row.id, "id")?;
            ensure_receipt_field(
                &receipt,
                receipt.project_id.as_str() == row.project_id,
                "project_id",
            )?;
            ensure_receipt_field(
                &receipt,
                optional_id(receipt.task_id.as_ref()) == row.task_id.as_deref(),
                "task_id",
            )?;
            ensure_receipt_field(
                &receipt,
                optional_id(receipt.run_id.as_ref()) == row.run_id.as_deref(),
                "run_id",
            )?;
            ensure_receipt_field(&receipt, receipt.hash == row.hash, "hash")?;
            ensure_receipt_field(
                &receipt,
                receipt.previous_hash == row.previous_hash,
                "previous_hash",
            )?;
            ensure_receipt_field(
                &receipt,
                timestamp(receipt.created_at) == row.created_at,
                "created_at",
            )?;
            if receipt.previous_hash != expected_previous {
                return Err(ReceiptChainError::PreviousHash {
                    id: receipt.id,
                    expected: expected_previous,
                    actual: receipt.previous_hash,
                }
                .into());
            }
            let expected = compute_receipt_hash(&ReceiptDraft::from(&receipt))?;
            if receipt.hash != expected {
                return Err(ReceiptChainError::HashMismatch {
                    id: receipt.id,
                    expected,
                    actual: receipt.hash,
                }
                .into());
            }
            expected_previous = Some(receipt.hash);
        }
        Ok(())
    }

    pub fn upsert_artifact(&self, item: &Artifact) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO artifacts(id,project_id,task_id,run_id,created_at,data) VALUES (?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![item.id.as_str(),item.project_id.as_str(),optional_id(item.task_id.as_ref()),optional_id(item.run_id.as_ref()),timestamp(item.created_at),data],
        )?;
        Ok(())
    }

    pub fn list_artifacts(&self, project_id: &EntityId) -> Result<Vec<Artifact>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM artifacts WHERE project_id=?1 ORDER BY created_at DESC",
            params![project_id.as_str()],
        )
    }

    pub fn get_artifact(&self, id: &EntityId) -> Result<Option<Artifact>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM artifacts WHERE id=?1",
            params![id.as_str()],
        )
    }

    pub fn upsert_worktree(&self, item: &Worktree) -> Result<(), StoreError> {
        let data = json(item)?;
        self.connection.lock().execute(
            "INSERT INTO worktrees(id,project_id,task_id,run_id,status,updated_at,data) VALUES (?1,?2,?3,?4,?5,?6,?7) \
             ON CONFLICT(id) DO UPDATE SET run_id=excluded.run_id,status=excluded.status,updated_at=excluded.updated_at,data=excluded.data",
            params![item.id.as_str(),item.project_id.as_str(),item.task_id.as_str(),optional_id(item.run_id.as_ref()),item.status.as_str(),timestamp(item.updated_at),data],
        )?;
        Ok(())
    }

    pub fn list_worktrees(&self, project_id: &EntityId) -> Result<Vec<Worktree>, StoreError> {
        list_json(
            &self.connection.lock(),
            "SELECT data FROM worktrees WHERE project_id=?1 ORDER BY updated_at DESC",
            params![project_id.as_str()],
        )
    }

    pub fn get_worktree(&self, id: &EntityId) -> Result<Option<Worktree>, StoreError> {
        get_json(
            &self.connection.lock(),
            "SELECT data FROM worktrees WHERE id=?1",
            params![id.as_str()],
        )
    }

    /// Append an immutable run event with the next per-run sequence number.
    pub fn append_run_event(
        &self,
        run_id: &EntityId,
        kind: impl Into<String>,
        payload: Value,
    ) -> Result<RunEvent, StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let sequence: u64 = transaction.query_row(
            "SELECT COALESCE(MAX(sequence),0)+1 FROM run_events WHERE run_id=?1",
            params![run_id.as_str()],
            |row| row.get(0),
        )?;
        let event = RunEvent {
            run_id: run_id.clone(),
            sequence,
            kind: kind.into(),
            payload,
            created_at: now(),
        };
        transaction.execute(
            "INSERT INTO run_events(run_id,sequence,kind,payload,created_at) VALUES (?1,?2,?3,?4,?5)",
            params![event.run_id.as_str(),event.sequence,event.kind,json(&event.payload)?,timestamp(event.created_at)],
        )?;
        transaction.commit()?;
        Ok(event)
    }

    pub fn list_run_events(
        &self,
        run_id: &EntityId,
        after_sequence: u64,
    ) -> Result<Vec<RunEvent>, StoreError> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            "SELECT sequence,kind,payload,created_at FROM run_events WHERE run_id=?1 AND sequence>?2 ORDER BY sequence",
        )?;
        let rows = statement.query_map(params![run_id.as_str(), after_sequence], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (sequence, kind, payload, created_at) = row?;
            events.push(RunEvent {
                run_id: run_id.clone(),
                sequence,
                kind,
                payload: serde_json::from_str(&payload)?,
                created_at: parse_timestamp(&created_at)?,
            });
        }
        Ok(events)
    }

    pub fn append_runtime_log(
        &self,
        project_id: Option<&EntityId>,
        run_id: Option<&EntityId>,
        level: &str,
        body: &str,
    ) -> Result<(), StoreError> {
        self.connection.lock().execute(
            "INSERT INTO runtime_logs(project_id,run_id,level,body,created_at) VALUES (?1,?2,?3,?4,?5)",
            params![project_id.map(EntityId::as_str),run_id.map(EntityId::as_str),level,body,timestamp(now())],
        )?;
        Ok(())
    }

    /// Remove detailed events and logs before `cutoff`. Permanent receipt rows are untouched.
    pub fn prune_runtime_data(&self, cutoff: Timestamp) -> Result<PruneReport, StoreError> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let cutoff = timestamp(cutoff);
        let events = transaction.execute(
            "DELETE FROM run_events WHERE created_at < ?1",
            params![cutoff],
        )?;
        let logs = transaction.execute(
            "DELETE FROM runtime_logs WHERE created_at < ?1",
            params![cutoff],
        )?;
        transaction.commit()?;
        Ok(PruneReport { events, logs })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneReport {
    pub events: usize,
    pub logs: usize,
}

#[derive(Debug)]
struct StoredReceipt {
    sequence: u64,
    id: String,
    project_id: String,
    task_id: Option<String>,
    run_id: Option<String>,
    hash: String,
    previous_hash: Option<String>,
    created_at: String,
    data: String,
}

fn ensure_receipt_field(
    receipt: &Receipt,
    matches: bool,
    field: &'static str,
) -> Result<(), StoreError> {
    if matches {
        Ok(())
    } else {
        Err(ReceiptChainError::StoredFieldMismatch {
            id: receipt.id.clone(),
            field,
        }
        .into())
    }
}

fn configure(connection: &Connection) -> Result<(), StoreError> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn integrity_check(connection: &Connection) -> Result<(), StoreError> {
    let result: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(StoreError::Integrity(result))
    }
}

fn schema_version(connection: &Connection) -> Result<u32, StoreError> {
    Ok(connection.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn migrate(connection: &mut Connection, from: u32) -> Result<(), StoreError> {
    if from < 1 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(MIGRATION_0001)?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.commit()?;
    }
    Ok(())
}

fn create_backup(
    connection: &Connection,
    source_path: &Path,
    version: u32,
) -> Result<PathBuf, StoreError> {
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ba-agent.db");
    let backup_path = source_path.with_file_name(format!("{file_name}.pre-v{version}.{stamp}.bak"));
    let mut destination = Connection::open(&backup_path)?;
    let backup = Backup::new(connection, &mut destination)?;
    backup.run_to_completion(100, Duration::from_millis(10), None)?;
    drop(backup);
    destination.close().map_err(|(_, error)| error)?;
    Ok(backup_path)
}

fn insert_receipt(
    transaction: &Transaction<'_>,
    receipt: &Receipt,
    sequence: u64,
) -> Result<(), StoreError> {
    transaction.execute(
        "INSERT INTO receipts(id,project_id,task_id,run_id,chain_sequence,hash,previous_hash,created_at,data) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![receipt.id.as_str(),receipt.project_id.as_str(),optional_id(receipt.task_id.as_ref()),optional_id(receipt.run_id.as_ref()),sequence,receipt.hash,receipt.previous_hash,timestamp(receipt.created_at),json(receipt)?],
    )?;
    Ok(())
}

fn optional_id(value: Option<&EntityId>) -> Option<&str> {
    value.map(EntityId::as_str)
}

fn timestamp(value: Timestamp) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn parse_timestamp(value: &str) -> Result<Timestamp, StoreError> {
    Ok(chrono::DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn json<T: Serialize + ?Sized>(value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(Into::into)
}

fn get_json<T: DeserializeOwned, P: Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<Option<T>, StoreError> {
    let data = connection
        .query_row(sql, params, |row| row.get::<_, String>(0))
        .optional()?;
    data.map(|data| serde_json::from_str(&data).map_err(Into::into))
        .transpose()
}

fn required_json<T: DeserializeOwned, P: Params>(
    connection: &Connection,
    sql: &str,
    params: P,
    entity: &'static str,
    id: &EntityId,
) -> Result<T, StoreError> {
    get_json(connection, sql, params)?.ok_or_else(|| StoreError::NotFound {
        entity,
        id: id.clone(),
    })
}

fn list_json<T: DeserializeOwned, P: Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<T>, StoreError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params, |row| row.get::<_, String>(0))?;
    let mut values = Vec::new();
    for row in rows {
        values.push(serde_json::from_str(&row?)?);
    }
    Ok(values)
}

fn delete_by_id(
    connection: &Connection,
    table: &'static str,
    id: &EntityId,
) -> Result<bool, StoreError> {
    let sql = match table {
        "projects" => "DELETE FROM projects WHERE id=?1",
        "workstreams" => "DELETE FROM workstreams WHERE id=?1",
        "milestones" => "DELETE FROM milestones WHERE id=?1",
        "tasks" => "DELETE FROM tasks WHERE id=?1",
        "agent_profiles" => "DELETE FROM agent_profiles WHERE id=?1",
        _ => return Err(StoreError::UnsupportedDelete(table)),
    };
    Ok(connection.execute(sql, params![id.as_str()])? > 0)
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("timestamp error: {0}")]
    Timestamp(#[from] chrono::ParseError),
    #[error(transparent)]
    Transition(#[from] ba_core::StateTransitionError),
    #[error(transparent)]
    ReceiptChain(#[from] ReceiptChainError),
    #[error("database integrity check failed: {0}")]
    Integrity(String),
    #[error("database schema {found} is newer than supported schema {supported}")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("{entity} {id} was not found")]
    NotFound { entity: &'static str, id: EntityId },
    #[error("deletion is not supported for table {0}")]
    UnsupportedDelete(&'static str),
    #[error("run status is not terminal: {0}")]
    InvalidTerminalRunStatus(String),
    #[error("invalid entity link: {0}")]
    InvalidEntityLink(&'static str),
}

#[cfg(test)]
mod tests;
