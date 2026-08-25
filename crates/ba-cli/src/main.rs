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
    Init(InitArgs),
    Doctor,
    Ui(UiArgs),
    Project(ProjectArgs),
    Workstream(WorkstreamArgs),
    Task(TaskArgs),
    Run(RunArgs),
    Dispatch(DispatchArgs),
    Approval(ApprovalArgs),
    Gate(GateArgs),
    Receipt(ReceiptArgs),
    Adapter(AdapterArgs),
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
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    List,
    Show {
        id: String,
    },
}

#[derive(Debug, Args)]
struct WorkstreamArgs {
    #[command(subcommand)]
    command: WorkstreamCommand,
}

#[derive(Debug, Subcommand)]
enum WorkstreamCommand {
    Add {
        project_id: String,
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },
    List {
        project_id: String,
    },
}

#[derive(Debug, Args)]
struct TaskArgs {
    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
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
    },
    List {
        project_id: String,
    },
    Status {
        id: String,
        status: String,
    },
}

#[derive(Debug, Args)]
struct RunArgs {
    #[command(subcommand)]
    command: RunCommand,
}

#[derive(Debug, Subcommand)]
enum RunCommand {
    List {
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long)]
        project_id: Option<String>,
    },
    Show {
        id: String,
    },
    Events {
        id: String,
        #[arg(long, default_value_t = 0)]
        after: u64,
    },
    Cancel {
        id: String,
    },
}

#[derive(Debug, Args)]
struct DispatchArgs {
    task_id: String,
    #[arg(long)]
    agent_profile_id: Option<String>,
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
    List,
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
    List {
        task_id: String,
    },
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
    List { project_id: String },
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
        Command::Task(args) => app::task(&paths, args.command, cli.json),
        Command::Run(args) => app::run(&paths, args.command, cli.json).await,
        Command::Dispatch(args) => {
            app::dispatch(
                &paths,
                &args.task_id,
                args.agent_profile_id.as_deref(),
                cli.json,
            )
            .await
        }
        Command::Approval(args) => app::approval(&paths, args.command, cli.json).await,
        Command::Gate(args) => app::gate(&paths, args.command, cli.json).await,
        Command::Receipt(args) => app::receipt(&paths, args.command, cli.json),
        Command::Adapter(args) => app::adapter(args.command, cli.json).await,
        Command::Migrate(args) => migration::run(&paths, args.command, cli.json),
        Command::Mcp => mcp_backend::serve(&paths).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
