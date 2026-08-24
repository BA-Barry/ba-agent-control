use std::{path::PathBuf, str::FromStr, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use ba_core::{EntityId, RunStatus, Task, TaskStatus, now};
use ba_mcp::{ToolBackend, ToolError};
use ba_runtime::{ApprovalDecision, CodexAdapter};
use ba_scheduler::{
    ApprovalResolution, DispatchCoordinator, FinalizationCoordinator, GateCoordinator,
    RuntimeCoordinator, configured_gate,
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
        control: RuntimeCoordinator::new(store.clone(), Arc::new(CodexAdapter::default())),
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
    async fn call_inner(&self, tool: &str, arguments: &Value) -> anyhow::Result<Value> {
        match tool {
            "project.list" => Ok(serde_json::to_value(self.store.list_projects()?)?),
            "project.get" => {
                let id = required_id(arguments, "project_id")?;
                let project = self.store.get_project(&id)?.context("project not found")?;
                let workstreams = self.store.list_workstreams(&id)?;
                let tasks = self.store.list_tasks(&id)?;
                let runs = self.store.list_project_runs(&id)?;
                Ok(json!({
                    "project": project,
                    "health": {
                        "workstreams": workstreams.len(),
                        "tasks": tasks.len(),
                        "runs": runs.len(),
                        "blocked": tasks.iter().filter(|item| item.status == TaskStatus::Blocked).count(),
                        "failedRuns": runs.iter().filter(|item| item.status == RunStatus::Failed).count()
                    }
                }))
            }
            "workstream.list" => Ok(serde_json::to_value(
                self.store
                    .list_workstreams(&required_id(arguments, "project_id")?)?,
            )?),
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
                Ok(serde_json::to_value(
                    self.store.transition_task(&id, status)?,
                )?)
            }
            "agent-profile.list" => Ok(serde_json::to_value(self.store.list_agent_profiles()?)?),
            "dispatch.create" => self.create_dispatch(arguments).await,
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
        let stamp = now();
        let task = Task {
            id: EntityId::new(),
            project_id: workstream.project_id,
            workstream_id: Some(workstream_id),
            milestone_id: None,
            title: required_string(arguments, "title")?.to_owned(),
            description: arguments
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            status: TaskStatus::Draft,
            write: false,
            path_scope: Vec::new(),
            depends_on: Vec::new(),
            created_at: stamp,
            updated_at: stamp,
        };
        self.store.upsert_task(&task)?;
        Ok(serde_json::to_value(task)?)
    }

    async fn create_dispatch(&self, arguments: &Value) -> anyhow::Result<Value> {
        let task_id = required_id(arguments, "task_id")?;
        let profile_id = required_id(arguments, "agent_profile_id")?;
        let bundle = DispatchCoordinator::new(self.store.clone(), self.worktree_root.clone())
            .enqueue(&task_id, Some(&profile_id))?;
        let bundle = self.control.launch(bundle, None).await?;
        Ok(serde_json::to_value(bundle)?)
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
            let _event = self.control.append_event(
                run_id,
                "worktree.committed",
                json!({"finalization": finalization}),
            )?;
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

fn required_id(arguments: &Value, key: &str) -> anyhow::Result<EntityId> {
    EntityId::from_str(required_string(arguments, key)?).map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ba_core::{Project, TaskStatus};

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
}
