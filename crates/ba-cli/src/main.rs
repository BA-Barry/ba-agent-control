mod app;
mod local_client;
mod mcp_backend;
mod migration;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "ba-agent-control", version, about)]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize `.ba-agent/` and register a Git project.
    Init(InitArgs),
    /// Check the database, local service, Git, Codex, Claude, and GitHub adapters.
    Doctor,
    /// Start or reopen the authenticated local console.
    Ui(UiArgs),
    /// Register and inspect local projects.
    Project(ProjectArgs),
    /// Create and inspect project workstreams.
    Workstream(WorkstreamArgs),
    /// Create, list, and transition tasks.
    Task(TaskArgs),
    /// Inspect and control runtime runs and events.
    Run(RunArgs),
    /// Dispatch tasks and manage the durable run queue and concurrency policy.
    Dispatch(DispatchArgs),
    /// Inspect and respond to protected-operation approvals.
    Approval(ApprovalArgs),
    /// Inspect and execute configured quality gates.
    Gate(GateArgs),
    /// List and verify permanent receipt chains.
    Receipt(ReceiptArgs),
    /// Inspect runtime, Agent profile, GitHub, and compatibility adapters.
    Adapter(AdapterArgs),
    /// Generate/import compatible configuration and evidence with dry-run defaults.
    Migrate(MigrateArgs),
    #[command(hide = true)]
    Mcp,
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    name: Option<String>,
}

#[derive(Debug, Args)]
struct UiArgs {
    #[arg(long, default_value_t = 0)]
    port: u16,
    #[arg(long)]
    no_open: bool,
}

#[derive(Debug, Args)]
struct ProjectArgs {
    #[command(subcommand)]
    command: ProjectCommand,
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    /// Register an existing local Git repository.
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Initialize BA Agent metadata and import an existing local Git root.
    Import {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "zh-CN")]
        language: String,
    },
    /// Create a local directory, initialize Git and BA Agent metadata, and register it.
    Create {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long, default_value = "zh-CN")]
        language: String,
    },
    /// List registered projects.
    List,
    /// Show one project by ULID.
    Show { id: String },
}

#[derive(Debug, Args)]
struct WorkstreamArgs {
    #[command(subcommand)]
    command: WorkstreamCommand,
}

#[derive(Debug, Subcommand)]
enum WorkstreamCommand {
    /// Add a workstream to a project.
    Add {
        project_id: String,
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// List a project's workstreams.
    List { project_id: String },
    /// List built-in templates and project-local overrides.
    Templates { project_id: String },
    /// Copy one built-in template into `.ba-agent/workflows/` for editing.
    Materialize {
        project_id: String,
        template_id: String,
    },
    /// Instantiate a template as one atomic assigned task graph.
    Instantiate {
        project_id: String,
        template_id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        milestone_id: Option<String>,
        #[arg(long = "scope")]
        write_scope: Vec<String>,
    },
    /// Start a bounded implement, test, review, repair, and confirmation loop.
    LoopStart {
        project_id: String,
        title: String,
        #[arg(long)]
        objective: String,
        #[arg(long = "scope", required = true)]
        write_scope: Vec<String>,
        #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(1..=3))]
        max_repairs: u8,
    },
    /// List a project's bounded review/repair loops.
    LoopList { project_id: String },
    /// Show one loop and refresh its status from durable tasks.
    LoopShow { id: String },
    /// Accept a review, request the next repair, or block the loop.
    LoopDecide {
        id: String,
        #[arg(value_parser = ["accept", "repair", "block"])]
        decision: String,
        #[arg(long)]
        note: String,
    },
}

#[derive(Debug, Args)]
struct TaskArgs {
    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    /// Create a task; write tasks require at least one repository-relative scope.
    Add {
        project_id: String,
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        workstream_id: Option<String>,
        #[arg(long)]
        milestone_id: Option<String>,
        #[arg(long = "depends-on")]
        depends_on: Vec<String>,
        #[arg(long)]
        write: bool,
        #[arg(long = "scope")]
        path_scope: Vec<String>,
        #[arg(long)]
        agent_profile_id: Option<String>,
    },
    /// List a project's active tasks, optionally including inactive history.
    List {
        project_id: String,
        #[arg(long)]
        include_inactive: bool,
    },
    /// Transition a task through its validated lifecycle.
    Status { id: String, status: String },
    /// Hide an obsolete task without changing its lifecycle status.
    Archive {
        id: String,
        #[arg(long)]
        reason: String,
    },
    /// Hide an obsolete task and preserve its same-project replacement reference.
    Supersede {
        id: String,
        #[arg(long)]
        replacement_task_id: String,
        #[arg(long)]
        reason: String,
    },
    /// Restore an archived or superseded task to the default active register.
    Restore {
        id: String,
        #[arg(long)]
        reason: String,
    },
    /// Read append-only archive, supersede, and restore decisions.
    DispositionHistory { id: String },
    /// Persist a structured handoff for directly dependent downstream tasks.
    HandoffCreate {
        source_task_id: String,
        #[arg(long)]
        source_run_id: Option<String>,
        #[arg(long = "to", required = true)]
        destination_task_ids: Vec<String>,
        #[arg(long)]
        outcome: String,
        #[arg(long = "decision")]
        decisions: Vec<String>,
        #[arg(long = "evidence")]
        evidence: Vec<String>,
        #[arg(long = "changed-path")]
        changed_paths: Vec<String>,
        #[arg(long = "command-evidence")]
        command_evidence: Vec<String>,
        #[arg(long = "risk")]
        risks: Vec<String>,
        #[arg(long = "next-step")]
        next_steps: Vec<String>,
        #[arg(long = "artifact-id")]
        artifact_ids: Vec<String>,
        #[arg(long = "receipt-id")]
        receipt_ids: Vec<String>,
    },
    /// List incoming or outgoing structured handoffs for one task.
    HandoffList {
        task_id: String,
        #[arg(long)]
        outgoing: bool,
    },
}

#[derive(Debug, Args)]
struct RunArgs {
    #[command(subcommand)]
    command: RunCommand,
}

#[derive(Debug, Subcommand)]
enum RunCommand {
    /// List runs for exactly one task or project.
    List {
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long)]
        project_id: Option<String>,
    },
    /// Show one run.
    Show { id: String },
    /// Read ordered events for one run.
    Events {
        id: String,
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
    /// Cancel a running or waiting run.
    Cancel { id: String },
}

#[derive(Debug, Args)]
struct DispatchArgs {
    #[command(subcommand)]
    command: Option<DispatchCommand>,
    task_id: Option<String>,
    #[arg(long)]
    agent_profile_id: Option<String>,
}

#[derive(Debug, Subcommand)]
enum DispatchCommand {
    /// List dispatches durably waiting for admission.
    Queue,
    /// Replace the exact waiting order with the supplied dispatch ULIDs.
    Reorder { dispatch_ids: Vec<String> },
    /// Cancel one queued dispatch.
    Cancel { id: String },
    /// Set the shared global maximum (1–64, default 4).
    Global { maximum: usize },
    /// Set a project's maximum; it cannot exceed the current global maximum.
    ProjectLimit { project_id: String, maximum: usize },
}

#[derive(Debug, Args)]
struct ApprovalArgs {
    #[command(subcommand)]
    command: ApprovalCommand,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ApprovalDecision {
    Approve,
    Reject,
    Cancel,
}

#[derive(Debug, Subcommand)]
enum ApprovalCommand {
    /// List pending approvals.
    List,
    /// Approve, reject, or cancel a pending approval.
    Respond {
        id: String,
        decision: ApprovalDecision,
        #[arg(long, default_value = "user decision")]
        reason: String,
    },
}

#[derive(Debug, Args)]
struct GateArgs {
    #[command(subcommand)]
    command: GateCommand,
}

#[derive(Debug, Subcommand)]
enum GateCommand {
    /// List quality gates configured for a task's project.
    List { task_id: String },
    /// Run one configured gate with its exact program and argument vector.
    Run {
        task_id: String,
        name: String,
        program: String,
        #[arg(last = true)]
        args: Vec<String>,
        #[arg(long)]
        run_id: Option<String>,
        #[arg(long, default_value = ".")]
        cwd: PathBuf,
        #[arg(long, default_value_t = 600_000)]
        timeout_ms: u64,
    },
}

#[derive(Debug, Args)]
struct ReceiptArgs {
    #[command(subcommand)]
    command: ReceiptCommand,
}

#[derive(Debug, Subcommand)]
enum ReceiptCommand {
    /// List permanent receipts for a project.
    List { project_id: String },
    /// Verify a project's complete forward hash chain.
    Verify { project_id: String },
}

#[derive(Debug, Args)]
struct AdapterArgs {
    #[command(subcommand)]
    command: AdapterCommand,
}

#[derive(Debug, Subcommand)]
enum AdapterCommand {
    List,
    /// List durable Agent profiles available for dispatch.
    Profiles,
    Probe {
        #[arg(default_value = "codex")]
        adapter: String,
    },
    Repository {
        /// GitHub repository in owner/name form.
        repository: String,
    },
    Compatibility {
        #[arg(default_value = "claude")]
        adapter: String,
        /// BA Agent plugin root containing agents, Skills, and Hooks.
        #[arg(long)]
        plugin_root: PathBuf,
    },
}

#[derive(Debug, Args)]
struct MigrateArgs {
    #[command(subcommand)]
    command: MigrateCommand,
}

#[derive(Debug, Subcommand)]
enum MigrateCommand {
    TheAgency {
        source: PathBuf,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        project_root: Option<PathBuf>,
    },
    Config {
        plugin_root: PathBuf,
        project_root: PathBuf,
        #[arg(long)]
        apply: bool,
    },
    RuntimeState {
        source: PathBuf,
        project_id: String,
        #[arg(long)]
        apply: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let paths = app::DataPaths::resolve(cli.data_dir)?;
    match cli.command {
        Command::Init(args) => app::init(&paths, &args.path, args.name, cli.json),
        Command::Doctor => app::doctor(&paths, cli.json).await,
        Command::Ui(args) => app::ui(&paths, args.port, args.no_open).await,
        Command::Project(args) => app::project(&paths, args.command, cli.json),
        Command::Workstream(args) => app::workstream(&paths, args.command, cli.json),
        Command::Task(args) => app::task(&paths, args.command, cli.json).await,
        Command::Run(args) => app::run(&paths, args.command, cli.json).await,
        Command::Dispatch(args) => app::dispatch(&paths, args, cli.json).await,
        Command::Approval(args) => app::approval(&paths, args.command, cli.json).await,
        Command::Gate(args) => app::gate(&paths, args.command, cli.json).await,
        Command::Receipt(args) => app::receipt(&paths, args.command, cli.json),
        Command::Adapter(args) => app::adapter(&paths, args.command, cli.json).await,
        Command::Migrate(args) => migration::run(&paths, args.command, cli.json),
        Command::Mcp => mcp_backend::serve(&paths).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_profiles_is_a_supported_command() -> anyhow::Result<()> {
        let cli = Cli::try_parse_from(["ba-agent-control", "adapter", "profiles"])?;
        assert!(matches!(
            cli.command,
            Command::Adapter(AdapterArgs {
                command: AdapterCommand::Profiles
            })
        ));
        Ok(())
    }

    #[test]
    fn task_add_accepts_dependency_milestone_and_write_scope_flags() -> anyhow::Result<()> {
        let project_id = "01K00000000000000000000000";
        let milestone_id = "01K00000000000000000000001";
        let dependency_id = "01K00000000000000000000002";
        let cli = Cli::try_parse_from([
            "ba-agent-control",
            "task",
            "add",
            project_id,
            "Implement dependency support",
            "--description",
            "Acceptance-oriented detail",
            "--milestone-id",
            milestone_id,
            "--depends-on",
            dependency_id,
            "--write",
            "--scope",
            "apps/console/**",
        ])?;

        let Command::Task(TaskArgs {
            command:
                TaskCommand::Add {
                    description,
                    milestone_id: parsed_milestone,
                    depends_on,
                    write,
                    path_scope,
                    ..
                },
        }) = cli.command
        else {
            anyhow::bail!("task add command was not parsed");
        };
        assert_eq!(description, "Acceptance-oriented detail");
        assert_eq!(parsed_milestone.as_deref(), Some(milestone_id));
        assert_eq!(depends_on, [dependency_id]);
        assert!(write);
        assert_eq!(path_scope, ["apps/console/**"]);
        Ok(())
    }

    #[test]
    fn task_disposition_commands_require_reason_and_replacement() -> anyhow::Result<()> {
        let task_id = "01K00000000000000000000000";
        let replacement_id = "01K00000000000000000000001";
        let listed = Cli::try_parse_from([
            "ba-agent-control",
            "task",
            "list",
            task_id,
            "--include-inactive",
        ])?;
        assert!(matches!(
            listed.command,
            Command::Task(TaskArgs {
                command: TaskCommand::List {
                    include_inactive: true,
                    ..
                }
            })
        ));
        let superseded = Cli::try_parse_from([
            "ba-agent-control",
            "task",
            "supersede",
            task_id,
            "--replacement-task-id",
            replacement_id,
            "--reason",
            "replaced by verified work",
        ])?;
        assert!(matches!(
            superseded.command,
            Command::Task(TaskArgs {
                command: TaskCommand::Supersede { replacement_task_id, reason, .. }
            }) if replacement_task_id == replacement_id && reason == "replaced by verified work"
        ));
        assert!(Cli::try_parse_from(["ba-agent-control", "task", "archive", task_id]).is_err());
        Ok(())
    }

    #[test]
    fn dispatch_keeps_positional_compatibility_and_adds_queue_controls() -> anyhow::Result<()> {
        let task_id = "01K00000000000000000000000";
        let direct = Cli::try_parse_from(["ba-agent-control", "dispatch", task_id])?;
        assert!(matches!(
            direct.command,
            Command::Dispatch(DispatchArgs { command: None, task_id: Some(id), .. }) if id == task_id
        ));
        let queued = Cli::try_parse_from(["ba-agent-control", "dispatch", "queue"])?;
        assert!(matches!(
            queued.command,
            Command::Dispatch(DispatchArgs {
                command: Some(DispatchCommand::Queue),
                ..
            })
        ));
        let limit = Cli::try_parse_from(["ba-agent-control", "dispatch", "global", "6"])?;
        assert!(matches!(
            limit.command,
            Command::Dispatch(DispatchArgs {
                command: Some(DispatchCommand::Global { maximum: 6 }),
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn project_onboarding_commands_distinguish_import_and_local_create() -> anyhow::Result<()> {
        let imported = Cli::try_parse_from([
            "ba-agent-control",
            "project",
            "import",
            "G:/existing",
            "--name",
            "Existing",
        ])?;
        assert!(matches!(
            imported.command,
            Command::Project(ProjectArgs {
                command: ProjectCommand::Import { name: Some(name), .. }
            }) if name == "Existing"
        ));
        let created = Cli::try_parse_from([
            "ba-agent-control",
            "project",
            "create",
            "G:/new-project",
            "--language",
            "en",
        ])?;
        assert!(matches!(
            created.command,
            Command::Project(ProjectArgs {
                command: ProjectCommand::Create { language, .. }
            }) if language == "en"
        ));
        Ok(())
    }

    #[test]
    fn workflow_template_and_handoff_commands_keep_assignments_explicit() -> anyhow::Result<()> {
        let workflow = Cli::try_parse_from([
            "ba-agent-control",
            "workstream",
            "instantiate",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "feature-delivery",
            "--scope",
            "crates/auth",
        ])?;
        assert!(matches!(
            workflow.command,
            Command::Workstream(WorkstreamArgs {
                command: WorkstreamCommand::Instantiate { template_id, write_scope, .. }
            }) if template_id == "feature-delivery" && write_scope == ["crates/auth"]
        ));

        let loop_start = Cli::try_parse_from([
            "ba-agent-control",
            "workstream",
            "loop-start",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "Authentication repair",
            "--objective",
            "Authentication state survives recovery",
            "--scope",
            "crates/auth",
            "--max-repairs",
            "3",
        ])?;
        assert!(matches!(
            loop_start.command,
            Command::Workstream(WorkstreamArgs {
                command: WorkstreamCommand::LoopStart { max_repairs: 3, write_scope, .. }
            }) if write_scope == ["crates/auth"]
        ));

        let loop_decide = Cli::try_parse_from([
            "ba-agent-control",
            "workstream",
            "loop-decide",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "repair",
            "--note",
            "P1 remains",
        ])?;
        assert!(matches!(
            loop_decide.command,
            Command::Workstream(WorkstreamArgs {
                command: WorkstreamCommand::LoopDecide { decision, note, .. }
            }) if decision == "repair" && note == "P1 remains"
        ));

        let handoff = Cli::try_parse_from([
            "ba-agent-control",
            "task",
            "handoff-create",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "--to",
            "01ARZ3NDEKTSV4RRFFQ69G5FAW",
            "--outcome",
            "Root cause confirmed",
        ])?;
        assert!(matches!(
            handoff.command,
            Command::Task(TaskArgs {
                command: TaskCommand::HandoffCreate { outcome, destination_task_ids, .. }
            }) if outcome == "Root cause confirmed" && destination_task_ids.len() == 1
        ));
        Ok(())
    }
}
