use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    str::FromStr,
};

use anyhow::{Context, bail};
use ba_config::{ProjectConfig, Runtime};
use ba_core::{EntityId, Project, Task, TaskStatus, Workstream, now};
use ba_git::RepoPath;
use ba_github::GitHubAdapter;
use ba_runtime::{CodexAdapter, RuntimeAdapter};
use ba_scheduler::{
    FinalizationCoordinator, GateCoordinator, GateDefinition, configured_gate,
    ensure_builtin_agent_profiles,
};
use ba_store::SqliteStore;
use directories::ProjectDirs;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    AdapterCommand, ApprovalCommand, ApprovalDecision, GateCommand, ProjectCommand, ReceiptCommand,
    RunCommand, TaskCommand, WorkstreamCommand,
};

#[derive(Clone, Debug)]
pub struct DataPaths {
    pub root: PathBuf,
    pub database: PathBuf,
    pub worktrees: PathBuf,
    pub logs: PathBuf,
    pub control: PathBuf,
}

impl DataPaths {
    pub fn resolve(override_root: Option<PathBuf>) -> anyhow::Result<Self> {
        let root = match override_root {
            Some(path) => path,
            None => ProjectDirs::from("dev", "BA Agent", "ba-agent-control")
                .context("unable to resolve the operating-system data directory")?
                .data_local_dir()
                .to_path_buf(),
        };
        fs::create_dir_all(&root)?;
        let worktrees = root.join("worktrees");
        let logs = root.join("logs");
        fs::create_dir_all(&worktrees)?;
        fs::create_dir_all(&logs)?;
        Ok(Self {
            database: root.join("ba-agent.db"),
            control: root.join("control.json"),
            root,
            worktrees,
            logs,
        })
    }

    pub fn store(&self) -> anyhow::Result<SqliteStore> {
        let store = SqliteStore::open(&self.database)?;
        ensure_builtin_agent_profiles(&store)?;
        Ok(store)
    }
}

pub fn init(
    paths: &DataPaths,
    path: &Path,
    name: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    let root = path.canonicalize().context("project root does not exist")?;
    ensure_git_repository(&root)?;
    let config_root = root.join(".ba-agent");
    for directory in ["workflows", "plans", "knowledge", "receipts"] {
        fs::create_dir_all(config_root.join(directory))?;
    }
    let name = name.unwrap_or_else(|| {
        root.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("project")
            .to_owned()
    });
    let project_file = config_root.join("project.toml");
    if !project_file.exists() {
        fs::write(
            &project_file,
            format!(
                "version = 1\nname = {name:?}\nlanguage = \"zh-CN\"\nlog_retention_days = 90\nmax_concurrency = 4\n\n[gates]\nformat = []\nlint = []\ntypecheck = []\nchanged_tests = []\nsecret_scan = []\ndiff_review = []\n"
            ),
        )?;
    }
    let project = register_project(paths, &root, Some(name))?;
    emit(&project, json)
}

pub async fn doctor(paths: &DataPaths, json_output: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    store.integrity_check()?;
    let codex = CodexAdapter::new("codex").probe().await?;
    let github = GitHubAdapter::default().probe().await;
    let report = json!({
        "status": if codex.available { "ready" } else { "attention" },
        "dataDirectory": paths.root,
        "database": paths.database,
        "worktreesDirectory": paths.worktrees,
        "logsDirectory": paths.logs,
        "schemaVersion": store.schema_version()?,
        "databaseIntegrity": "ok",
        "git": version_output("git", &["--version"]),
        "codex": codex,
        "claude": version_output("claude", &["--version"]),
        "github": github,
        "telemetry": false,
        "binding": "127.0.0.1"
    });
    emit(&report, json_output)
}

pub async fn ui(paths: &DataPaths, port: u16, no_open: bool) -> anyhow::Result<()> {
    let mut config = ba_server::ServerConfig::new(paths.database.clone());
    config.port = port;
    let server = ba_server::start(config).await?;
    println!("BA Agent: {}", server.launch_url);
    if !no_open {
        open_browser(&server.launch_url);
    }
    tokio::signal::ctrl_c().await?;
    server.shutdown().await
}

pub fn project(paths: &DataPaths, command: ProjectCommand, json: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    match command {
        ProjectCommand::Add { path, name } => emit(&register_project(paths, &path, name)?, json),
        ProjectCommand::List => emit(&store.list_projects()?, json),
        ProjectCommand::Show { id } => {
            let item = store
                .get_project(&id_of(&id)?)?
                .context("project not found")?;
            emit(&item, json)
        }
    }
}

pub fn workstream(paths: &DataPaths, command: WorkstreamCommand, json: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    match command {
        WorkstreamCommand::Add {
            project_id,
            name,
            description,
        } => {
            let project_id = id_of(&project_id)?;
            store
                .get_project(&project_id)?
                .context("project not found")?;
            let stamp = now();
            let item = Workstream {
                id: EntityId::new(),
                project_id,
                name,
                description,
                status: "active".into(),
                created_at: stamp,
                updated_at: stamp,
            };
            store.upsert_workstream(&item)?;
            emit(&item, json)
        }
        WorkstreamCommand::List { project_id } => {
            emit(&store.list_workstreams(&id_of(&project_id)?)?, json)
        }
    }
}

pub fn task(paths: &DataPaths, command: TaskCommand, json: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    match command {
        TaskCommand::Add {
            project_id,
            title,
            description,
            workstream_id,
            milestone_id,
            depends_on,
            write,
            path_scope,
        } => {
            if title.trim().is_empty() {
                bail!("task title is required");
            }
            if write && path_scope.is_empty() {
                bail!("write tasks require at least one --scope");
            }
            if !write && !path_scope.is_empty() {
                bail!("read-only tasks cannot declare --scope; add --write or remove the scope");
            }
            for scope in &path_scope {
                RepoPath::new(scope).with_context(|| format!("invalid write scope: {scope}"))?;
            }
            let project_id = id_of(&project_id)?;
            store
                .get_project(&project_id)?
                .context("project not found")?;
            let workstream_id = workstream_id.as_deref().map(id_of).transpose()?;
            if let Some(id) = &workstream_id {
                let workstream = store.get_workstream(id)?.context("workstream not found")?;
                if workstream.project_id != project_id {
                    bail!("workstream belongs to a different project");
                }
            }
            let milestone_id = milestone_id.as_deref().map(id_of).transpose()?;
            if let Some(id) = &milestone_id {
                let milestone = store.get_milestone(id)?.context("milestone not found")?;
                if milestone.project_id != project_id {
                    bail!("milestone belongs to a different project");
                }
                if milestone.workstream_id.is_some() && milestone.workstream_id != workstream_id {
                    bail!("milestone belongs to a different workstream");
                }
            }
            let depends_on = depends_on
                .iter()
                .map(|id| id_of(id))
                .collect::<anyhow::Result<Vec<_>>>()?;
            for dependency_id in &depends_on {
                let dependency = store
                    .get_task(dependency_id)?
                    .context("dependency task not found")?;
                if dependency.project_id != project_id {
                    bail!("dependency task belongs to a different project");
                }
            }
            let stamp = now();
            let item = Task {
                id: EntityId::new(),
                project_id,
                workstream_id,
                milestone_id,
                title: title.trim().to_owned(),
                description,
                status: TaskStatus::Draft,
                write,
                path_scope,
                depends_on,
                created_at: stamp,
                updated_at: stamp,
            };
            store.upsert_task(&item)?;
            emit(&item, json)
        }
        TaskCommand::List { project_id } => emit(&store.list_tasks(&id_of(&project_id)?)?, json),
        TaskCommand::Status { id, status } => {
            let status = TaskStatus::from_str(&status)?;
            emit(&store.transition_task(&id_of(&id)?, status)?, json)
        }
    }
}

pub async fn run(paths: &DataPaths, command: RunCommand, json: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    match command {
        RunCommand::List {
            task_id,
            project_id,
        } => {
            let values = match (task_id, project_id) {
                (Some(id), None) => store.list_runs(&id_of(&id)?)?,
                (None, Some(id)) => store.list_project_runs(&id_of(&id)?)?,
                _ => bail!("provide exactly one of --task-id or --project-id"),
            };
            emit(&values, json)
        }
        RunCommand::Show { id } => emit(
            &store.get_run(&id_of(&id)?)?.context("run not found")?,
            json,
        ),
        RunCommand::Events { id, after } => {
            emit(&store.list_run_events(&id_of(&id)?, after)?, json)
        }
        RunCommand::Cancel { id } => {
            let id = id_of(&id)?;
            let value =
                crate::local_client::post(paths, &format!("/api/v1/runs/{id}/cancel"), json!({}))
                    .await?;
            emit(&value, json)
        }
    }
}

pub async fn dispatch(
    paths: &DataPaths,
    task_id: &str,
    agent_profile_id: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let task_id = id_of(task_id)?;
    let profile_id = agent_profile_id.map(id_of).transpose()?;
    let value = crate::local_client::post(
        paths,
        "/api/v1/dispatches",
        json!({
            "taskId": task_id,
            "agentProfileId": profile_id,
        }),
    )
    .await?;
    emit(&value, json)
}

pub async fn approval(
    paths: &DataPaths,
    command: ApprovalCommand,
    json: bool,
) -> anyhow::Result<()> {
    let store = paths.store()?;
    match command {
        ApprovalCommand::List => emit(&store.list_pending_approvals()?, json),
        ApprovalCommand::Respond {
            id,
            decision,
            reason,
        } => {
            let decision = match decision {
                ApprovalDecision::Approve => "approved",
                ApprovalDecision::Reject => "denied",
                ApprovalDecision::Cancel => "cancelled",
            };
            let id = id_of(&id)?;
            let value = crate::local_client::post(
                paths,
                &format!("/api/v1/approvals/{id}/respond"),
                json!({"decision": decision, "reason": reason}),
            )
            .await?;
            emit(&value, json)
        }
    }
}

pub async fn gate(paths: &DataPaths, command: GateCommand, json: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    match command {
        GateCommand::List { task_id } => emit(&store.list_gates(&id_of(&task_id)?)?, json),
        GateCommand::Run {
            task_id,
            name,
            program,
            args,
            run_id,
            cwd,
            timeout_ms,
        } => {
            let cwd = cwd.canonicalize()?;
            let task_id = id_of(&task_id)?;
            let task = store.get_task(&task_id)?.context("task not found")?;
            let project = store
                .get_project(&task.project_id)?
                .context("project not found")?;
            let project_root = PathBuf::from(project.root_path).canonicalize()?;
            let configured = configured_gate(&project_root, &name)?;
            ensure_gate_command_matches(&configured, &program, &args)?;
            let definition = GateDefinition {
                name,
                program,
                args,
                timeout_ms,
            };
            let run_id = run_id.as_deref().map(id_of).transpose()?;
            let gate = GateCoordinator::new(store.clone())
                .run(&task_id, run_id.as_ref(), &cwd, &definition)
                .await?;
            let finalization = FinalizationCoordinator::new(store, paths.worktrees.clone())
                .finalize_if_eligible(&task_id, run_id.as_ref())?;
            emit(&json!({"gate": gate, "finalization": finalization}), json)
        }
    }
}

fn ensure_gate_command_matches(
    configured: &GateDefinition,
    program: &str,
    args: &[String],
) -> anyhow::Result<()> {
    if configured.program != program || configured.args != args {
        bail!(
            "quality gate command does not match the project configuration for {}",
            configured.name
        );
    }
    Ok(())
}

pub fn receipt(paths: &DataPaths, command: ReceiptCommand, json: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    match command {
        ReceiptCommand::List { project_id } => {
            emit(&store.list_receipts(&id_of(&project_id)?)?, json)
        }
        ReceiptCommand::Verify { project_id } => {
            store.verify_receipt_chain(&id_of(&project_id)?)?;
            emit(&json!({"valid": true}), json)
        }
    }
}

pub async fn adapter(command: AdapterCommand, json: bool) -> anyhow::Result<()> {
    match command {
        AdapterCommand::List => emit(
            &json!([
                {"id":"codex","mode":"live","preferred":"app-server","fallback":"exec-json"},
                {"id":"claude","mode":"configuration-only"},
                {"id":"github","mode":"optional-read-only","credentialPrecedence":["gh_cli","os_credential_store","environment"]}
            ]),
            json,
        ),
        AdapterCommand::Probe { adapter } if adapter == "codex" => {
            emit(&CodexAdapter::new("codex").probe().await?, json)
        }
        AdapterCommand::Probe { adapter } if adapter == "claude" => emit(
            &json!({"adapter":"claude","mode":"configuration-only","version":version_output("claude", &["--version"])}),
            json,
        ),
        AdapterCommand::Probe { adapter } if adapter == "github" => {
            emit(&GitHubAdapter::default().probe().await, json)
        }
        AdapterCommand::Repository { repository } => emit(
            &GitHubAdapter::default()
                .view_repository(&repository)
                .await?,
            json,
        ),
        AdapterCommand::Compatibility {
            adapter,
            plugin_root,
        } if adapter == "codex" || adapter == "claude" => {
            let runtime = if adapter == "codex" {
                Runtime::Codex
            } else {
                Runtime::Claude
            };
            let config = ProjectConfig::from_plugin(
                plugin_root,
                "BA Agent manages this repository through checksum-safe generated configuration.",
            )?;
            emit(&config.compatibility_report(runtime)?, json)
        }
        AdapterCommand::Compatibility { adapter, .. } | AdapterCommand::Probe { adapter } => {
            bail!("unknown adapter: {adapter}")
        }
    }
}

fn register_project(
    paths: &DataPaths,
    path: &Path,
    name: Option<String>,
) -> anyhow::Result<Project> {
    let root = path.canonicalize().context("project root does not exist")?;
    ensure_git_repository(&root)?;
    let stamp = now();
    let item = Project {
        id: EntityId::new(),
        name: name.unwrap_or_else(|| {
            root.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("project")
                .to_owned()
        }),
        root_path: root.to_string_lossy().into_owned(),
        description: String::new(),
        language: "zh-CN".into(),
        created_at: stamp,
        updated_at: stamp,
    };
    paths.store()?.upsert_project(&item)?;
    Ok(item)
}

fn ensure_git_repository(root: &Path) -> anyhow::Result<()> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        bail!("not a Git repository: {}", root.display());
    }
    Ok(())
}

fn id_of(value: &str) -> anyhow::Result<EntityId> {
    EntityId::from_str(value).map_err(anyhow::Error::from)
}

fn version_output(program: &str, args: &[&str]) -> Value {
    ProcessCommand::new(program)
        .args(args)
        .output()
        .map_or_else(
            |error| json!({"available":false,"error":error.to_string()}),
            |output| {
                json!({
                    "available": output.status.success(),
                    "version": String::from_utf8_lossy(&output.stdout).trim(),
                    "error": String::from_utf8_lossy(&output.stderr).trim()
                })
            },
        )
}

fn open_browser(url: &str) {
    let result = if cfg!(target_os = "windows") {
        ProcessCommand::new("explorer.exe").arg(url).spawn()
    } else if cfg!(target_os = "macos") {
        ProcessCommand::new("open").arg(url).spawn()
    } else {
        ProcessCommand::new("xdg-open").arg(url).spawn()
    };
    if let Err(error) = result {
        tracing::warn!(%error, "unable to open browser automatically");
    }
}

pub fn emit<T: Serialize>(value: &T, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_gate_name_cannot_be_paired_with_a_different_command() {
        let configured = GateDefinition {
            name: "lint".into(),
            program: "cargo".into(),
            args: vec!["clippy".into()],
            timeout_ms: 600_000,
        };

        assert!(ensure_gate_command_matches(&configured, "cargo", &["clippy".into()]).is_ok());
        assert!(ensure_gate_command_matches(&configured, "git", &["--version".into()]).is_err());
    }
}
