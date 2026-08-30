use std::{path::PathBuf, str::FromStr, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use ba_core::{EntityId, Milestone, RunStatus, Task, TaskStatus, Timestamp, now};
use ba_git::RepoPath;
use ba_mcp::{ToolBackend, ToolError};
use ba_runtime::{ApprovalDecision, CodexAdapter};
use ba_scheduler::{
    ApprovalResolution, DispatchCoordinator, FinalizationCoordinator, GateCoordinator,
    ProjectOnboardingMode, ProjectOnboardingRequest, ProjectOnboardingService, RuntimeCoordinator,
    SchedulerPolicyService, configured_gate,
};
use ba_store::SqliteStore;
use serde_json::{Value, json};

use crate::app::DataPaths;

#[derive(Debug)]
struct StoreBackend {
    store: SqliteStore,
    worktree_root: PathBuf,
    control: RuntimeCoordinator,
}

pub async fn serve(paths: &DataPaths) -> anyhow::Result<()> {
    let store = paths.store()?;
    let backend = StoreBackend {
        control: RuntimeCoordinator::new(store.clone(), Arc::new(CodexAdapter::default()))
            .with_queue(paths.worktrees.clone()),
        store,
        worktree_root: paths.worktrees.clone(),
    };
    ba_mcp::serve_stdio(Arc::new(backend)).await
}

#[async_trait]
impl ToolBackend for StoreBackend {
    async fn call(&self, tool: &str, arguments: Value) -> Result<Value, ToolError> {
        self.call_inner(tool, &arguments)
            .await
            .map_err(|error| ToolError::internal(error.to_string()))
    }
}

impl StoreBackend {
    #[allow(
        clippy::too_many_lines,
        reason = "the MCP name-to-domain router is clearer as one exhaustive match"
    )]
    async fn call_inner(&self, tool: &str, arguments: &Value) -> anyhow::Result<Value> {
        match tool {
            "project.list" => Ok(serde_json::to_value(self.store.list_projects()?)?),
            "project.get" => {
                let id = required_id(arguments, "project_id")?;
                let project = self.store.get_project(&id)?.context("project not found")?;
                let workstreams = self.store.list_workstreams(&id)?;
                let milestones = self.store.list_milestones(&id)?;
                let tasks = self.store.list_tasks(&id)?;
                let runs = self.store.list_project_runs(&id)?;
                Ok(json!({
                    "project": project,
                    "health": {
                        "workstreams": workstreams.len(),
                        "milestones": milestones.len(),
                        "tasks": tasks.len(),
                        "runs": runs.len(),
                        "blocked": tasks.iter().filter(|item| item.status == TaskStatus::Blocked).count(),
                        "failedRuns": runs.iter().filter(|item| item.status == RunStatus::Failed).count()
                    }
                }))
            }
            "project.import" => self.onboard_project(arguments, ProjectOnboardingMode::Import),
            "project.create" => self.onboard_project(arguments, ProjectOnboardingMode::Create),
            "workstream.list" => Ok(serde_json::to_value(
                self.store
                    .list_workstreams(&required_id(arguments, "project_id")?)?,
            )?),
            "milestone.list" => Ok(serde_json::to_value(
                self.store
                    .list_milestones(&required_id(arguments, "project_id")?)?,
            )?),
            "milestone.create" => self.create_milestone(arguments),
            "task.list" => {
                let workstream_id = required_id(arguments, "workstream_id")?;
                let workstream = self
                    .store
                    .get_workstream(&workstream_id)?
                    .context("workstream not found")?;
                let tasks = self
                    .store
                    .list_tasks(&workstream.project_id)?
                    .into_iter()
                    .filter(|task| task.workstream_id.as_ref() == Some(&workstream_id))
                    .collect::<Vec<_>>();
                Ok(serde_json::to_value(tasks)?)
            }
            "task.create" => self.create_task(arguments),
            "task.transition" => {
                let id = required_id(arguments, "task_id")?;
                let status = TaskStatus::from_str(required_string(arguments, "status")?)?;
                let task = self.store.transition_task(&id, status)?;
                if status == TaskStatus::Completed {
                    self.control.drain_queue().await?;
                }
                Ok(serde_json::to_value(task)?)
            }
            "agent-profile.list" => Ok(serde_json::to_value(self.store.list_agent_profiles()?)?),
            "dispatch.create" => self.create_dispatch(arguments).await,
            "dispatch.queue" | "dispatch.reorder" | "dispatch.cancel" => {
                self.dispatch_queue_control(tool, arguments).await
            }
            "scheduler.get" | "scheduler.set" | "scheduler.project.set" => {
                self.scheduler_control(tool, arguments).await
            }
            "run.get" => {
                let id = required_id(arguments, "run_id")?;
                let run = self.store.get_run(&id)?.context("run not found")?;
                let latest_sequence = self
                    .store
                    .list_run_events(&id, 0)?
                    .last()
                    .map_or(0, |event| event.sequence);
                Ok(json!({"run": run, "latestSequence": latest_sequence}))
            }
            "run.events" => {
                let after = arguments.get("after").and_then(Value::as_u64).unwrap_or(0);
                Ok(serde_json::to_value(self.store.list_run_events(
                    &required_id(arguments, "run_id")?,
                    after,
                )?)?)
            }
            "run.message" => {
                let run_id = required_id(arguments, "run_id")?;
                let message = required_string(arguments, "message")?;
                let event = self.control.message(&run_id, message).await?;
                Ok(serde_json::to_value(event)?)
            }
            "run.cancel" => {
                let (run, event) = self
                    .control
                    .cancel(&required_id(arguments, "run_id")?, "mcp_user")
                    .await?;
                Ok(json!({"run": run, "event": event}))
            }
            "gate.list" => Ok(serde_json::to_value(
                self.store.list_gates(&required_id(arguments, "task_id")?)?,
            )?),
            "gate.run" => self.run_gate(arguments).await,
            "receipt.verify" => {
                self.store
                    .verify_receipt_chain(&required_id(arguments, "project_id")?)?;
                Ok(json!({"valid": true}))
            }
            "approval.list" => Ok(serde_json::to_value(self.store.list_pending_approvals()?)?),
            "approval.respond" => self.respond_approval(arguments).await,
            _ => anyhow::bail!("unsupported MCP tool: {tool}"),
        }
    }

    fn create_task(&self, arguments: &Value) -> anyhow::Result<Value> {
        let workstream_id = required_id(arguments, "workstream_id")?;
        let workstream = self
            .store
            .get_workstream(&workstream_id)?
            .context("workstream not found")?;
        let project_id = workstream.project_id;
        let milestone_id = optional_id(arguments, "milestone_id")?;
        if let Some(id) = &milestone_id {
            let milestone = self
                .store
                .get_milestone(id)?
                .context("milestone not found")?;
            if milestone.project_id != project_id {
                anyhow::bail!("milestone belongs to a different project");
            }
            if milestone.workstream_id.is_some()
                && milestone.workstream_id.as_ref() != Some(&workstream_id)
            {
                anyhow::bail!("milestone belongs to a different workstream");
            }
        }
        let write = optional_bool(arguments, "write")?.unwrap_or(false);
        let path_scope = optional_string_array(arguments, "path_scope")?;
        if write && path_scope.is_empty() {
            anyhow::bail!("write tasks require at least one path_scope entry");
        }
        if !write && !path_scope.is_empty() {
            anyhow::bail!("read-only tasks cannot declare path_scope");
        }
        for scope in &path_scope {
            RepoPath::new(scope).with_context(|| format!("invalid write scope: {scope}"))?;
        }
        let depends_on = optional_string_array(arguments, "depends_on")?
            .iter()
            .map(|id| EntityId::from_str(id).map_err(anyhow::Error::from))
            .collect::<anyhow::Result<Vec<_>>>()?;
        for dependency_id in &depends_on {
            let dependency = self
                .store
                .get_task(dependency_id)?
                .context("dependency task not found")?;
            if dependency.project_id != project_id {
                anyhow::bail!("dependency task belongs to a different project");
            }
        }
        let stamp = now();
        let task = Task {
            id: EntityId::new(),
            project_id,
            workstream_id: Some(workstream_id),
            milestone_id,
            title: required_string(arguments, "title")?.to_owned(),
            description: arguments
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            status: TaskStatus::Draft,
            write,
            path_scope,
            depends_on,
            created_at: stamp,
            updated_at: stamp,
        };
        self.store.upsert_task(&task)?;
        Ok(serde_json::to_value(task)?)
    }

    fn onboard_project(
        &self,
        arguments: &Value,
        mode: ProjectOnboardingMode,
    ) -> anyhow::Result<Value> {
        let path = PathBuf::from(required_string(arguments, "path")?);
        anyhow::ensure!(path.is_absolute(), "project path must be absolute");
        let result = ProjectOnboardingService::new(self.store.clone()).onboard(
            ProjectOnboardingRequest {
                mode,
                path,
                name: optional_string(arguments, "name"),
                description: optional_string(arguments, "description").unwrap_or_default(),
                language: optional_string(arguments, "language").unwrap_or_else(|| "zh-CN".into()),
            },
        )?;
        Ok(serde_json::to_value(result)?)
    }

    fn create_milestone(&self, arguments: &Value) -> anyhow::Result<Value> {
        let project_id = required_id(arguments, "project_id")?;
        self.store
            .get_project(&project_id)?
            .context("project not found")?;
        let workstream_id = optional_id(arguments, "workstream_id")?;
        if let Some(id) = &workstream_id {
            let workstream = self
                .store
                .get_workstream(id)?
                .context("workstream not found")?;
            if workstream.project_id != project_id {
                anyhow::bail!("workstream belongs to a different project");
            }
        }
        let status = arguments
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("planned")
            .trim()
            .to_ascii_lowercase();
        if !matches!(status.as_str(), "planned" | "active") {
            anyhow::bail!("milestone.create only accepts planned or active status");
        }
        let due_at = arguments
            .get("due_at")
            .and_then(Value::as_str)
            .map(Timestamp::from_str)
            .transpose()?;
        let stamp = now();
        let milestone = Milestone {
            id: EntityId::new(),
            project_id,
            workstream_id,
            name: required_string(arguments, "name")?.trim().to_owned(),
            description: arguments
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            status,
            due_at,
            created_at: stamp,
            updated_at: stamp,
        };
        self.store.create_milestone(&milestone)?;
        Ok(serde_json::to_value(milestone)?)
    }

    async fn create_dispatch(&self, arguments: &Value) -> anyhow::Result<Value> {
        let task_id = required_id(arguments, "task_id")?;
        let profile_id = required_id(arguments, "agent_profile_id")?;
        let bundle = DispatchCoordinator::new(self.store.clone(), self.worktree_root.clone())
            .enqueue_with_prompt(
                &task_id,
                Some(&profile_id),
                arguments
                    .get("prompt")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            )?;
        self.control.drain_queue().await?;
        let run = self
            .store
            .get_run(&bundle.run.id)?
            .context("run not found")?;
        let dispatch = self
            .store
            .get_dispatch(&bundle.dispatch.id)?
            .context("dispatch not found")?;
        Ok(json!({"run": run, "dispatch": dispatch}))
    }

    async fn dispatch_queue_control(&self, tool: &str, arguments: &Value) -> anyhow::Result<Value> {
        match tool {
            "dispatch.queue" => Ok(serde_json::to_value(
                self.store.list_dispatches_by_status("queued")?,
            )?),
            "dispatch.reorder" => {
                let ids = required_id_array(arguments, "dispatch_ids")?;
                Ok(serde_json::to_value(
                    self.store.reorder_queued_dispatches(&ids)?,
                )?)
            }
            "dispatch.cancel" => {
                let dispatch_id = required_id(arguments, "dispatch_id")?;
                let dispatch = self
                    .store
                    .get_dispatch(&dispatch_id)?
                    .context("dispatch not found")?;
                let (run, event) = self.control.cancel(&dispatch.run_id, "mcp_user").await?;
                Ok(json!({"dispatchId": dispatch.id, "run": run, "event": event}))
            }
            _ => anyhow::bail!("unsupported dispatch queue tool: {tool}"),
        }
    }

    async fn scheduler_control(&self, tool: &str, arguments: &Value) -> anyhow::Result<Value> {
        let service = SchedulerPolicyService::new(self.store.clone());
        match tool {
            "scheduler.get" => self.scheduler_policy(),
            "scheduler.set" => {
                service.set_global(required_usize(arguments, "max_concurrency")?)?;
                self.control.drain_queue().await?;
                self.scheduler_policy()
            }
            "scheduler.project.set" => {
                let project_id = required_id(arguments, "project_id")?;
                let project = self
                    .store
                    .get_project(&project_id)?
                    .context("project not found")?;
                let policy =
                    service.set_project(&project, required_usize(arguments, "max_concurrency")?)?;
                self.control.drain_queue().await?;
                Ok(serde_json::to_value(policy)?)
            }
            _ => anyhow::bail!("unsupported scheduler tool: {tool}"),
        }
    }

    fn scheduler_policy(&self) -> anyhow::Result<Value> {
        let service = SchedulerPolicyService::new(self.store.clone());
        let global = service.global()?;
        let projects = self
            .store
            .list_projects()?
            .iter()
            .map(|project| service.project(project))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({"globalMaxConcurrency": global.global_max_concurrency, "projects": projects}))
    }

    async fn run_gate(&self, arguments: &Value) -> anyhow::Result<Value> {
        let task_id = required_id(arguments, "task_id")?;
        let task = self.store.get_task(&task_id)?.context("task not found")?;
        let project = self
            .store
            .get_project(&task.project_id)?
            .context("project not found")?;
        let project_root = PathBuf::from(project.root_path).canonicalize()?;
        let definition = configured_gate(&project_root, required_string(arguments, "gate")?)?;
        let run_id = arguments
            .get("run_id")
            .and_then(Value::as_str)
            .map(EntityId::from_str)
            .transpose()?;
        if let Some(id) = &run_id {
            let run = self.store.get_run(id)?.context("run not found")?;
            if run.task_id != task_id {
                anyhow::bail!("run belongs to a different task");
            }
        }
        let gate = GateCoordinator::new(self.store.clone())
            .run(&task_id, run_id.as_ref(), &project_root, &definition)
            .await?;
        if let Some(run_id) = &run_id {
            let _event =
                self.control
                    .append_event(run_id, "gate.completed", json!({"gate": &gate}))?;
        }
        let finalization =
            FinalizationCoordinator::new(self.store.clone(), self.worktree_root.clone())
                .finalize_if_eligible(&task_id, run_id.as_ref())?;
        if let (Some(run_id), Some(finalization)) = (&run_id, &finalization) {
            let event = if finalization.git.is_some() {
                "worktree.committed"
            } else {
                "task.completed"
            };
            let _event =
                self.control
                    .append_event(run_id, event, json!({"finalization": finalization}))?;
        }
        if finalization.is_some() {
            self.control.drain_queue().await?;
        }
        Ok(json!({"gate": gate, "finalization": finalization}))
    }

    async fn respond_approval(&self, arguments: &Value) -> anyhow::Result<Value> {
        let (resolution, runtime_decision) = match required_string(arguments, "decision")? {
            "approved" => (ApprovalResolution::Approve, ApprovalDecision::Accept),
            "approved_for_session" => (
                ApprovalResolution::Approve,
                ApprovalDecision::AcceptForSession,
            ),
            "denied" => (ApprovalResolution::Reject, ApprovalDecision::Decline),
            "cancelled" => (ApprovalResolution::Cancel, ApprovalDecision::Cancel),
            value => anyhow::bail!("unsupported approval decision: {value}"),
        };
        let reason = arguments
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("MCP user decision");
        let evidence = json!({"reason": reason, "actor": "mcp_user"});
        let (approval, event) = self
            .control
            .respond_approval(
                &required_id(arguments, "approval_id")?,
                resolution,
                runtime_decision,
                &evidence,
            )
            .await?;
        Ok(json!({"approval": approval, "event": event}))
    }
}

fn required_string<'a>(arguments: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("missing or empty argument: {key}"))
}

fn optional_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required_id(arguments: &Value, key: &str) -> anyhow::Result<EntityId> {
    EntityId::from_str(required_string(arguments, key)?).map_err(anyhow::Error::from)
}

fn required_id_array(arguments: &Value, key: &str) -> anyhow::Result<Vec<EntityId>> {
    arguments
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("missing or invalid array argument: {key}"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .with_context(|| format!("{key} entries must be strings"))
                .and_then(|value| EntityId::from_str(value).map_err(anyhow::Error::from))
        })
        .collect()
}

fn required_usize(arguments: &Value, key: &str) -> anyhow::Result<usize> {
    let value = arguments
        .get(key)
        .and_then(Value::as_u64)
        .with_context(|| format!("missing or invalid integer argument: {key}"))?;
    usize::try_from(value).with_context(|| format!("argument is too large: {key}"))
}

fn optional_id(arguments: &Value, key: &str) -> anyhow::Result<Option<EntityId>> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(EntityId::from_str)
        .transpose()
        .map_err(anyhow::Error::from)
}

fn optional_bool(arguments: &Value, key: &str) -> anyhow::Result<Option<bool>> {
    arguments
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .with_context(|| format!("argument must be a boolean: {key}"))
        })
        .transpose()
}

fn optional_string_array(arguments: &Value, key: &str) -> anyhow::Result<Vec<String>> {
    arguments
        .get(key)
        .map(|value| {
            value
                .as_array()
                .with_context(|| format!("argument must be an array: {key}"))?
                .iter()
                .map(|entry| {
                    entry
                        .as_str()
                        .filter(|text| !text.trim().is_empty())
                        .map(str::to_owned)
                        .with_context(|| format!("{key} entries must be non-empty strings"))
                })
                .collect()
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ba_core::{Project, TaskStatus, Workstream};

    #[tokio::test]
    async fn gate_tool_executes_the_project_configured_command() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let config_directory = directory.path().join(".ba-agent");
        std::fs::create_dir_all(&config_directory)?;
        std::fs::write(
            config_directory.join("project.toml"),
            "[gates]\ndoctor = [\"rustc\", \"--version\"]\n",
        )?;
        let store = SqliteStore::open(directory.path().join("state.sqlite3"))?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "MCP gate fixture".into(),
            root_path: directory.path().to_string_lossy().into_owned(),
            description: String::new(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        let task = Task {
            id: EntityId::new(),
            project_id: project.id,
            workstream_id: None,
            milestone_id: None,
            title: "Run configured gate".into(),
            description: String::new(),
            status: TaskStatus::Ready,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_task(&task)?;
        let backend = StoreBackend {
            control: RuntimeCoordinator::new(store.clone(), Arc::new(CodexAdapter::default())),
            store,
            worktree_root: directory.path().join("worktrees"),
        };

        let result = backend
            .call_inner("gate.run", &json!({"task_id": task.id, "gate": "doctor"}))
            .await?;
        assert_eq!(result["gate"]["status"], "passed");
        assert!(
            result["gate"]["result"]["stdout"]
                .as_str()
                .is_some_and(|output| output.contains("rustc"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn milestone_tools_persist_and_report_real_milestones() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let store = SqliteStore::open(directory.path().join("state.sqlite3"))?;
        let stamp = now();
        let project = Project {
            id: EntityId::new(),
            name: "MCP milestone fixture".into(),
            root_path: directory.path().to_string_lossy().into_owned(),
            description: "not used as milestone state".into(),
            language: "en".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_project(&project)?;
        let workstream = Workstream {
            id: EntityId::new(),
            project_id: project.id.clone(),
            name: "Alpha delivery".into(),
            description: String::new(),
            status: "active".into(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_workstream(&workstream)?;
        let dependency = Task {
            id: EntityId::new(),
            project_id: project.id.clone(),
            workstream_id: Some(workstream.id.clone()),
            milestone_id: None,
            title: "Define acceptance boundary".into(),
            description: String::new(),
            status: TaskStatus::Completed,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        store.upsert_task(&dependency)?;
        let backend = StoreBackend {
            control: RuntimeCoordinator::new(store.clone(), Arc::new(CodexAdapter::default())),
            store,
            worktree_root: directory.path().join("worktrees"),
        };

        let created = backend
            .call_inner(
                "milestone.create",
                &json!({
                    "project_id": project.id,
                    "workstream_id": workstream.id,
                    "name": "Personal Alpha",
                    "description": "Acceptance boundary",
                    "status": "active",
                    "due_at": "2026-08-31T00:00:00Z"
                }),
            )
            .await?;
        assert_eq!(created["name"], "Personal Alpha");
        assert_eq!(created["status"], "active");
        let listed = backend
            .call_inner("milestone.list", &json!({"project_id": project.id}))
            .await?;
        assert_eq!(listed.as_array().map(Vec::len), Some(1));
        assert_eq!(listed[0]["id"], created["id"]);

        let task = backend
            .call_inner(
                "task.create",
                &json!({
                    "workstream_id": workstream.id,
                    "milestone_id": created["id"],
                    "title": "Ship dependency-aware task",
                    "description": "Created through MCP",
                    "write": true,
                    "path_scope": ["apps/console/**"],
                    "depends_on": [dependency.id]
                }),
            )
            .await?;
        assert_eq!(task["write"], true);
        assert_eq!(task["path_scope"], json!(["apps/console/**"]));
        assert_eq!(task["depends_on"], json!([dependency.id]));
        assert_eq!(task["milestone_id"], created["id"]);
        Ok(())
    }
}
