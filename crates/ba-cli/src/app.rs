use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    str::FromStr,
};

use anyhow::{Context, bail};
use ba_config::{ProjectConfig, Runtime};
use ba_core::{EntityId, Task, TaskStatus, Workstream, now};
use ba_git::RepoPath;
use ba_github::GitHubAdapter;
use ba_runtime::{CodexAdapter, RuntimeAdapter};
use ba_scheduler::{
    GateDefinition, HandoffCommandEvidence, HandoffDraft, HandoffService, ProjectOnboardingMode,
    ProjectOnboardingRequest, ProjectOnboardingService, ReviewRepairAction,
    ReviewRepairLoopDecisionRequest, ReviewRepairLoopService, ReviewRepairLoopStartRequest,
    TaskDispositionService, WorkflowInstantiationRequest, WorkflowTemplateService, configured_gate,
    ensure_builtin_agent_profiles,
};
use ba_store::SqliteStore;
use directories::ProjectDirs;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    AdapterCommand, ApprovalCommand, ApprovalDecision, DispatchArgs, DispatchCommand, GateCommand,
    ProjectCommand, ReceiptCommand, RunCommand, TaskCommand, WorkstreamCommand,
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
        Self::resolve_with_env(override_root, std::env::var_os("BA_AGENT_DATA_DIR"))
    }

    fn resolve_with_env(
        override_root: Option<PathBuf>,
        environment_root: Option<OsString>,
    ) -> anyhow::Result<Self> {
        let root = match override_root {
            Some(path) => path,
            None => match environment_root.filter(|value| !value.is_empty()) {
                Some(value) => {
                    let path = PathBuf::from(value);
                    if !path.is_absolute() {
                        bail!("BA_AGENT_DATA_DIR must be an absolute path");
                    }
                    path
                }
                None => ProjectDirs::from("dev", "BA Agent", "ba-agent-control")
                    .context("unable to resolve the operating-system data directory")?
                    .data_local_dir()
                    .to_path_buf(),
            },
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
    let result =
        ProjectOnboardingService::new(paths.store()?).onboard(ProjectOnboardingRequest {
            mode: ProjectOnboardingMode::Import,
            path: path.to_path_buf(),
            name,
            description: String::new(),
            language: "zh-CN".into(),
        })?;
    emit(&result.project, json)
}

pub async fn doctor(paths: &DataPaths, json_output: bool) -> anyhow::Result<()> {
    let store = paths.store()?;
    store.integrity_check()?;
    let codex = CodexAdapter::new("codex").probe().await?;
    let github = GitHubAdapter::default().probe().await;
    let control_service = match ba_server::inspect_control_descriptor(&paths.control).await {
        ba_server::ControlDescriptorInspection::Live(descriptor) => json!({
            "status": "running",
            "origin": descriptor.origin,
            "processId": descriptor.process_id,
            "startedAt": descriptor.started_at,
            "recovery": "run `ba-agent-control ui` to reopen the authenticated console"
        }),
        ba_server::ControlDescriptorInspection::Missing => json!({
            "status": "stopped",
            "recovery": "run `ba-agent-control ui` to start the local console"
        }),
        ba_server::ControlDescriptorInspection::Stale => json!({
            "status": "stale",
            "recovery": "run `ba-agent-control ui`; the stale descriptor will be replaced after authenticated health verification"
        }),
    };
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
        "controlService": control_service,
        "telemetry": false,
        "binding": "127.0.0.1"
    });
    emit(&report, json_output)
}

pub async fn ui(paths: &DataPaths, port: u16, no_open: bool) -> anyhow::Result<()> {
    if port == 0
        && let Some(descriptor) = ba_server::live_control_descriptor(&paths.control).await
    {
        let launch_url = format!(
            "{}/session?token={}",
            descriptor.origin, descriptor.session_token
        );
        println!("BA Agent: {launch_url} (existing service)");
        if !no_open {
            open_browser(&launch_url);
        }
        return Ok(());
    }
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
        ProjectCommand::Add { path, name } => {
            let result =
                ProjectOnboardingService::new(store).onboard(ProjectOnboardingRequest {
                    mode: ProjectOnboardingMode::Import,
                    path,
                    name,
                    description: String::new(),
                    language: "zh-CN".into(),
                })?;
            emit(&result.project, json)
        }
        ProjectCommand::Import {
            path,
            name,
            description,
            language,
        } => emit(
            &ProjectOnboardingService::new(store).onboard(ProjectOnboardingRequest {
                mode: ProjectOnboardingMode::Import,
                path,
                name,
                description,
                language,
            })?,
            json,
        ),
        ProjectCommand::Create {
            path,
            name,
            description,
            language,
        } => emit(
            &ProjectOnboardingService::new(store).onboard(ProjectOnboardingRequest {
                mode: ProjectOnboardingMode::Create,
                path,
                name,
                description,
                language,
            })?,
            json,
        ),
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
        WorkstreamCommand::Templates { project_id } => emit(
            &WorkflowTemplateService::new(store).catalog(&id_of(&project_id)?)?,
            json,
        ),
        WorkstreamCommand::Materialize {
            project_id,
            template_id,
        } => emit(
            &json!({
                "path": WorkflowTemplateService::new(store)
                    .materialize(&id_of(&project_id)?, &template_id)?
            }),
            json,
        ),
        WorkstreamCommand::Instantiate {
            project_id,
            template_id,
            name,
            milestone_id,
            write_scope,
        } => emit(
            &WorkflowTemplateService::new(store).instantiate(&WorkflowInstantiationRequest {
                project_id: id_of(&project_id)?,
                template_id,
                workstream_name: name,
                milestone_id: milestone_id.as_deref().map(id_of).transpose()?,
                write_scope,
            })?,
            json,
        ),
        WorkstreamCommand::LoopStart {
            project_id,
            title,
            objective,
            write_scope,
            max_repairs,
        } => emit(
            &ReviewRepairLoopService::new(store).start(&ReviewRepairLoopStartRequest {
                project_id: id_of(&project_id)?,
                title,
                objective,
                write_scope,
                max_repairs,
            })?,
            json,
        ),
        WorkstreamCommand::LoopList { project_id } => emit(
            &ReviewRepairLoopService::new(store).list(&id_of(&project_id)?)?,
            json,
        ),
        WorkstreamCommand::LoopShow { id } => emit(
            &ReviewRepairLoopService::new(store).get(&id_of(&id)?)?,
            json,
        ),
        WorkstreamCommand::LoopDecide { id, decision, note } => {
            let action = match decision.as_str() {
                "accept" => ReviewRepairAction::Accept,
                "repair" => ReviewRepairAction::Repair,
                "block" => ReviewRepairAction::Block,
                _ => bail!("decision must be accept, repair, or block"),
            };
            emit(
                &ReviewRepairLoopService::new(store).decide(
                    &id_of(&id)?,
                    &ReviewRepairLoopDecisionRequest { action, note },
                )?,
                json,
            )
        }
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the CLI task dispatcher keeps each subcommand's validation and persistence path visible"
)]
pub async fn task(paths: &DataPaths, command: TaskCommand, json: bool) -> anyhow::Result<()> {
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
            agent_profile_id,
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
            let agent_profile_id = agent_profile_id.as_deref().map(id_of).transpose()?;
            if let Some(profile_id) = &agent_profile_id {
                let profile = store
                    .get_agent_profile(profile_id)?
                    .context("agent profile not found")?;
                if write && profile.config["sandboxMode"] != "workspace-write" {
                    bail!("assigned agent profile cannot execute write tasks");
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
                agent_profile_id,
                created_at: stamp,
                updated_at: stamp,
            };
            store.upsert_task(&item)?;
            emit(&item, json)
        }
        TaskCommand::List {
            project_id,
            include_inactive,
        } => {
            let project_id = id_of(&project_id)?;
            let tasks = if include_inactive {
                store.list_tasks(&project_id)?
            } else {
                TaskDispositionService::new(store).active_tasks(&project_id)?
            };
            emit(&tasks, json)
        }
        TaskCommand::Status { id, status } => {
            let status = TaskStatus::from_str(&status)?;
            let id = id_of(&id)?;
            let value = crate::local_client::post(
                paths,
                &format!("/api/v1/tasks/{id}/transition"),
                json!({"status": status.as_str()}),
            )
            .await?;
            emit(&value, json)
        }
        TaskCommand::Archive { id, reason } => emit(
            &TaskDispositionService::new(store).archive(&id_of(&id)?, &reason)?,
            json,
        ),
        TaskCommand::Supersede {
            id,
            replacement_task_id,
            reason,
        } => emit(
            &TaskDispositionService::new(store).supersede(
                &id_of(&id)?,
                &id_of(&replacement_task_id)?,
                &reason,
            )?,
            json,
        ),
        TaskCommand::Restore { id, reason } => emit(
            &TaskDispositionService::new(store).restore(&id_of(&id)?, &reason)?,
            json,
        ),
        TaskCommand::DispositionHistory { id } => emit(
            &TaskDispositionService::new(store).history(&id_of(&id)?)?,
            json,
        ),
        TaskCommand::HandoffCreate {
            source_task_id,
            source_run_id,
            destination_task_ids,
            outcome,
            decisions,
            evidence,
            changed_paths,
            command_evidence,
            risks,
            next_steps,
            artifact_ids,
            receipt_ids,
        } => {
            let commands = command_evidence
                .into_iter()
                .map(|value| {
                    let (command, result) = value.split_once("=>").ok_or_else(|| {
                        anyhow::anyhow!("--command-evidence must use `command => observed result`")
                    })?;
                    Ok(HandoffCommandEvidence {
                        command: command.trim().into(),
                        result: result.trim().into(),
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            emit(
                &HandoffService::new(store).create(HandoffDraft {
                    source_task_id: id_of(&source_task_id)?,
                    source_run_id: source_run_id.as_deref().map(id_of).transpose()?,
                    destination_task_ids: ids_of(&destination_task_ids)?,
                    outcome,
                    decisions,
                    evidence,
                    changed_paths,
                    commands,
                    risks,
                    next_steps,
                    artifact_ids: ids_of(&artifact_ids)?,
                    receipt_ids: ids_of(&receipt_ids)?,
                })?,
                json,
            )
        }
        TaskCommand::HandoffList { task_id, outgoing } => {
            let service = HandoffService::new(store);
            let task_id = id_of(&task_id)?;
            let records = if outgoing {
                service.list_from_task(&task_id)?
            } else {
                service.list_for_task(&task_id)?
            };
            emit(&records, json)
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

pub async fn dispatch(paths: &DataPaths, args: DispatchArgs, json: bool) -> anyhow::Result<()> {
    let value = match args.command {
        Some(DispatchCommand::Queue) => {
            crate::local_client::get(paths, "/api/v1/dispatches/queue").await?
        }
        Some(DispatchCommand::Reorder { dispatch_ids }) => {
            let ids = dispatch_ids
                .iter()
                .map(|id| id_of(id))
                .collect::<Result<Vec<_>, _>>()?;
            crate::local_client::post(
                paths,
                "/api/v1/dispatches/queue/reorder",
                json!({"dispatchIds": ids}),
            )
            .await?
        }
        Some(DispatchCommand::Cancel { id }) => {
            let id = id_of(&id)?;
            crate::local_client::post(paths, &format!("/api/v1/dispatches/{id}/cancel"), json!({}))
                .await?
        }
        Some(DispatchCommand::Global { maximum }) => {
            crate::local_client::post(
                paths,
                "/api/v1/scheduler",
                json!({"maxConcurrency": maximum}),
            )
            .await?
        }
        Some(DispatchCommand::ProjectLimit {
            project_id,
            maximum,
        }) => {
            let project_id = id_of(&project_id)?;
            crate::local_client::post(
                paths,
                &format!("/api/v1/scheduler/projects/{project_id}"),
                json!({"maxConcurrency": maximum}),
            )
            .await?
        }
        None => {
            let task_id = args
                .task_id
                .context("task id is required unless a dispatch subcommand is used")?;
            let task_id = id_of(&task_id)?;
            let profile_id = args.agent_profile_id.as_deref().map(id_of).transpose()?;
            crate::local_client::post(
                paths,
                "/api/v1/dispatches",
                json!({"taskId": task_id, "agentProfileId": profile_id}),
            )
            .await?
        }
    };
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
            let _cwd = cwd.canonicalize()?;
            let task_id = id_of(&task_id)?;
            let task = store.get_task(&task_id)?.context("task not found")?;
            let project = store
                .get_project(&task.project_id)?
                .context("project not found")?;
            let project_root = PathBuf::from(project.root_path).canonicalize()?;
            let configured = configured_gate(&project_root, &name)?;
            ensure_gate_command_matches(&configured, &program, &args)?;
            let _definition = GateDefinition {
                name: name.clone(),
                program,
                args,
                timeout_ms,
            };
            let run_id = run_id.as_deref().map(id_of).transpose()?;
            let response_timeout = std::time::Duration::from_millis(timeout_ms)
                .saturating_add(std::time::Duration::from_secs(5));
            let value = crate::local_client::post_with_timeout(
                paths,
                "/api/v1/gates/run",
                json!({"taskId": task_id, "runId": run_id, "gate": name}),
                response_timeout,
            )
            .await?;
            emit(&value, json)
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

pub async fn adapter(paths: &DataPaths, command: AdapterCommand, json: bool) -> anyhow::Result<()> {
    match command {
        AdapterCommand::List => emit(
            &json!([
                {"id":"codex","mode":"live","preferred":"app-server","fallback":"exec-json"},
                {"id":"claude","mode":"configuration-only"},
                {"id":"github","mode":"optional-read-only","credentialPrecedence":["gh_cli","os_credential_store","environment"]}
            ]),
            json,
        ),
        AdapterCommand::Profiles => emit(&paths.store()?.list_agent_profiles()?, json),
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

fn id_of(value: &str) -> anyhow::Result<EntityId> {
    EntityId::from_str(value).map_err(anyhow::Error::from)
}

fn ids_of(values: &[String]) -> anyhow::Result<Vec<EntityId>> {
    values.iter().map(|value| id_of(value)).collect()
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
    fn explicit_data_directory_overrides_environment() -> anyhow::Result<()> {
        let temporary = tempfile::tempdir()?;
        let explicit = temporary.path().join("explicit");
        let paths = DataPaths::resolve_with_env(
            Some(explicit.clone()),
            Some(OsString::from("relative-environment")),
        )?;

        assert_eq!(paths.root, explicit);
        assert!(paths.worktrees.is_dir());
        assert!(paths.logs.is_dir());
        Ok(())
    }

    #[test]
    fn environment_data_directory_is_used_without_explicit_override() -> anyhow::Result<()> {
        let temporary = tempfile::tempdir()?;
        let environment = temporary.path().join("environment");
        let paths = DataPaths::resolve_with_env(None, Some(environment.clone().into_os_string()))?;

        assert_eq!(paths.root, environment);
        assert_eq!(paths.database, environment.join("ba-agent.db"));
        assert_eq!(paths.control, environment.join("control.json"));
        Ok(())
    }

    #[test]
    fn relative_environment_data_directory_is_rejected() {
        assert!(matches!(
            DataPaths::resolve_with_env(None, Some(OsString::from("relative-data"))),
            Err(error) if error.to_string().contains("BA_AGENT_DATA_DIR must be an absolute path")
        ));
    }

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
