use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

use ba_core::{
    ApprovalStatus, Dispatch, EntityId, Gate, GateStatus, Project, Run, RunStatus, Task,
    TaskStatus, now,
};
use ba_git::GitOperation;
use ba_runtime::{CodexVersion, MINIMUM_CODEX_VERSION, RuntimeCapabilities};
use ba_store::SqliteStore;
use serde_json::json;

use crate::{
    ApprovalCoordinator, ApprovalResolution, DispatchCoordinator, FinalizationCoordinator,
    GateCoordinator, GateDefinition, GateEligibility, GateRunner, RecoveryAction,
    RecoveryCoordinator, ScheduleBlocker, Scheduler, SchedulerConfig, configured_gate,
    configured_gate_names, ensure_builtin_agent_profiles, evaluate_required_gates,
};

fn task(project_id: &EntityId, title: &str, write: bool, paths: &[&str]) -> Task {
    let timestamp = now();
    Task {
        id: EntityId::new(),
        project_id: project_id.clone(),
        workstream_id: None,
        milestone_id: None,
        title: title.to_owned(),
        description: String::new(),
        status: TaskStatus::Ready,
        write,
        path_scope: paths.iter().map(|path| (*path).to_owned()).collect(),
        depends_on: Vec::new(),
        created_at: timestamp,
        updated_at: timestamp,
    }
}

fn missing(message: &'static str) -> std::io::Error {
    std::io::Error::other(message)
}

#[test]
fn configured_gate_loads_an_argv_array_without_shell_parsing()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let config_directory = directory.path().join(".ba-agent");
    std::fs::create_dir_all(&config_directory)?;
    std::fs::write(
        config_directory.join("project.toml"),
        "[gates]\ncheck = [\"cargo\", \"test\", \"--workspace\"]\n",
    )?;

    let definition = configured_gate(directory.path(), "check")?;
    assert_eq!(definition.program, "cargo");
    assert_eq!(definition.args, ["test", "--workspace"]);
    assert_eq!(definition.timeout_ms, 600_000);
    assert_eq!(configured_gate_names(directory.path())?, ["check"]);
    assert!(configured_gate(directory.path(), "missing").is_err());
    Ok(())
}

#[test]
fn builtin_agent_profiles_are_complete_and_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let store = SqliteStore::open(directory.path().join("profiles.sqlite3"))?;
    let first = ensure_builtin_agent_profiles(&store)?;
    let second = ensure_builtin_agent_profiles(&store)?;
    assert_eq!(first.len(), 8);
    assert_eq!(second.len(), 8);
    assert_eq!(store.list_agent_profiles()?.len(), 8);
    assert!(first.iter().any(|profile| profile.role == "coordinator"));
    assert!(first.iter().any(|profile| profile.role == "implementer"));
    assert!(
        first
            .iter()
            .all(|profile| profile.config["builtin"] == true)
    );
    Ok(())
}

#[test]
fn scheduler_enforces_four_way_capacity_and_writer_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let project_id = EntityId::new();
    let mut writer = task(&project_id, "writer", true, &["src"]);
    let conflicting = task(&project_id, "conflict", true, &["src/api"]);
    let independent = task(&project_id, "independent", true, &["docs"]);
    let reader = task(&project_id, "reader", false, &[]);
    let mut scheduler = Scheduler::new(SchedulerConfig::default())?;
    scheduler.admit(&writer, &[writer.clone()])?;

    let plan = scheduler.plan(&[conflicting.clone(), independent.clone(), reader.clone()]);
    assert!(!plan.accepted.contains(&conflicting.id));
    assert!(plan.accepted.contains(&independent.id));
    assert!(plan.accepted.contains(&reader.id));
    assert!(plan.blocked[&conflicting.id]
        .iter()
        .any(|blocker| matches!(blocker, ScheduleBlocker::WriteConflict { running_task_id } if running_task_id == &writer.id)));

    writer.status = TaskStatus::Running;
    scheduler.release(&writer.id)?;
    let reads = (0..5)
        .map(|index| task(&project_id, &format!("read-{index}"), false, &[]))
        .collect::<Vec<_>>();
    let capacity_plan = scheduler.plan(&reads);
    assert_eq!(capacity_plan.accepted.len(), 4);
    assert_eq!(capacity_plan.blocked.len(), 1);
    assert!(
        capacity_plan
            .blocked
            .values()
            .flatten()
            .any(|blocker| matches!(blocker, ScheduleBlocker::ConcurrencyLimit { limit: 4 }))
    );
    Ok(())
}

#[test]
fn scheduler_blocks_dependency_failures_cycles_and_unsafe_scopes()
-> Result<(), Box<dyn std::error::Error>> {
    let project_id = EntityId::new();
    let mut first = task(&project_id, "first", false, &[]);
    let mut second = task(&project_id, "second", false, &[]);
    first.depends_on.push(second.id.clone());
    second.depends_on.push(first.id.clone());
    let mut unsafe_task = task(&project_id, "unsafe", true, &["../outside"]);
    unsafe_task.depends_on.push(EntityId::new());
    let scheduler = Scheduler::new(SchedulerConfig::default())?;
    let plan = scheduler.plan(&[first.clone(), second, unsafe_task.clone()]);
    assert!(
        plan.blocked[&first.id]
            .iter()
            .any(|blocker| matches!(blocker, ScheduleBlocker::DependencyCycle))
    );
    assert!(
        plan.blocked[&unsafe_task.id]
            .iter()
            .any(|blocker| matches!(blocker, ScheduleBlocker::MissingDependency { .. }))
    );
    assert!(
        plan.blocked[&unsafe_task.id]
            .iter()
            .any(|blocker| matches!(blocker, ScheduleBlocker::InvalidPathScope { .. }))
    );
    Ok(())
}

struct StoreFixture {
    directory: tempfile::TempDir,
    store: SqliteStore,
    project_id: EntityId,
    task_id: EntityId,
    run_id: EntityId,
}

fn store_fixture(status: RunStatus) -> Result<StoreFixture, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let store = SqliteStore::open(directory.path().join("state.sqlite3"))?;
    let timestamp = now();
    let project_id = EntityId::new();
    store.upsert_project(&Project {
        id: project_id.clone(),
        name: "test".to_owned(),
        root_path: directory.path().to_string_lossy().into_owned(),
        description: String::new(),
        language: "en".to_owned(),
        created_at: timestamp,
        updated_at: timestamp,
    })?;
    let task = task(&project_id, "persisted", false, &[]);
    let task_id = task.id.clone();
    store.upsert_task(&task)?;
    let run_id = EntityId::new();
    store.upsert_run(&Run {
        id: run_id.clone(),
        task_id: task_id.clone(),
        agent_profile_id: None,
        status,
        adapter: "codex".to_owned(),
        runtime_run_id: None,
        runtime_thread_id: Some("thread-test".to_owned()),
        started_at: Some(timestamp),
        finished_at: None,
        created_at: timestamp,
        updated_at: timestamp,
    })?;
    Ok(StoreFixture {
        directory,
        store,
        project_id,
        task_id,
        run_id,
    })
}

#[test]
fn dispatch_coordinator_persists_a_queued_run_and_event() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = store_fixture(RunStatus::Running)?;
    let candidate = task(&fixture.project_id, "queued", false, &[]);
    fixture.store.upsert_task(&candidate)?;
    let bundle = DispatchCoordinator::new(
        fixture.store.clone(),
        fixture.directory.path().join("worktrees"),
    )
    .enqueue(&candidate.id, None)?;

    assert_eq!(bundle.run.status, RunStatus::Queued);
    assert!(bundle.worktree.is_none());
    assert_eq!(
        fixture
            .store
            .get_task(&candidate.id)?
            .ok_or_else(|| missing("queued task missing"))?
            .status,
        TaskStatus::Queued
    );
    assert_eq!(fixture.store.list_run_events(&bundle.run.id, 0)?.len(), 1);
    Ok(())
}

#[test]
fn dispatch_coordinator_isolates_write_tasks_in_a_detached_worktree()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = store_fixture(RunStatus::Running)?;
    let repository = fixture.directory.path();
    run_git(repository, &["init", "-b", "main"])?;
    run_git(
        repository,
        &["config", "user.email", "test@example.invalid"],
    )?;
    run_git(repository, &["config", "user.name", "BA Agent Test"])?;
    std::fs::create_dir(repository.join("src"))?;
    std::fs::write(repository.join("src/base.txt"), "base\n")?;
    run_git(repository, &["add", "src/base.txt"])?;
    run_git(repository, &["commit", "-m", "base"])?;

    let worktrees = tempfile::tempdir()?;
    let candidate = task(&fixture.project_id, "writer", true, &["src"]);
    fixture.store.upsert_task(&candidate)?;
    let bundle = DispatchCoordinator::new(fixture.store.clone(), worktrees.path().to_path_buf())
        .enqueue(&candidate.id, None)?;
    let isolated = bundle
        .worktree
        .ok_or_else(|| missing("write-task worktree missing"))?;
    assert_eq!(isolated.run_id.as_ref(), Some(&bundle.run.id));
    assert!(std::path::Path::new(&isolated.path).is_dir());
    Ok(())
}

#[tokio::test]
async fn passed_configured_gates_finalize_local_commit_and_receipt()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = store_fixture(RunStatus::Running)?;
    let repository = fixture.directory.path();
    run_git(repository, &["init", "-b", "main"])?;
    run_git(
        repository,
        &["config", "user.email", "test@example.invalid"],
    )?;
    run_git(repository, &["config", "user.name", "BA Agent Test"])?;
    std::fs::create_dir_all(repository.join("src"))?;
    std::fs::create_dir_all(repository.join(".ba-agent"))?;
    std::fs::write(repository.join("src/base.txt"), "base\n")?;
    std::fs::write(
        repository.join(".ba-agent/project.toml"),
        "[gates]\ncheck = [\"git\", \"diff\", \"--check\"]\n",
    )?;
    std::fs::write(
        repository.join(".gitignore"),
        "state.sqlite3\nstate.sqlite3-*\n",
    )?;
    run_git(
        repository,
        &["add", ".gitignore", ".ba-agent", "src/base.txt"],
    )?;
    run_git(repository, &["commit", "-m", "base"])?;

    let worktrees = tempfile::tempdir()?;
    let candidate = task(&fixture.project_id, "writer", true, &["src"]);
    fixture.store.upsert_task(&candidate)?;
    let bundle = DispatchCoordinator::new(fixture.store.clone(), worktrees.path().to_path_buf())
        .enqueue(&candidate.id, None)?;
    let worktree = bundle
        .worktree
        .ok_or_else(|| missing("write-task worktree missing"))?;
    std::fs::write(
        PathBuf::from(&worktree.path).join("src/feature.txt"),
        "feature\n",
    )?;
    fixture
        .store
        .transition_task(&candidate.id, TaskStatus::Running)?;
    fixture
        .store
        .transition_task(&candidate.id, TaskStatus::Review)?;
    let definition = configured_gate(repository, "check")?;
    let gate = GateCoordinator::new(fixture.store.clone())
        .run(
            &candidate.id,
            Some(&bundle.run.id),
            Path::new(&worktree.path),
            &definition,
        )
        .await?;
    assert_eq!(gate.status, GateStatus::Passed);

    let finalized =
        FinalizationCoordinator::new(fixture.store.clone(), worktrees.path().to_path_buf())
            .finalize_if_eligible(&candidate.id, Some(&bundle.run.id))?
            .ok_or_else(|| missing("eligible worktree was not finalized"))?;
    assert!(finalized.git.branch.starts_with("ba/default/"));
    assert_eq!(
        finalized.worktree.status,
        ba_core::WorktreeStatus::Committed
    );
    assert_eq!(
        fixture
            .store
            .get_task(&candidate.id)?
            .ok_or_else(|| missing("finalized task missing"))?
            .status,
        TaskStatus::Completed
    );
    fixture.store.verify_receipt_chain(&candidate.project_id)?;
    assert_eq!(fixture.store.list_receipts(&candidate.project_id)?.len(), 1);
    Ok(())
}

fn run_git(directory: &std::path::Path, args: &[&str]) -> Result<(), std::io::Error> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

#[test]
fn protected_operations_pause_and_resume_only_after_all_approvals()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = store_fixture(RunStatus::Running)?;
    let coordinator = ApprovalCoordinator::new(fixture.store.clone());
    assert!(
        coordinator
            .request_git_operation(&fixture.run_id, GitOperation::LocalCommit, &json!({}))?
            .is_none()
    );
    let first = coordinator
        .request_git_operation(
            &fixture.run_id,
            GitOperation::Push,
            &json!({ "remote": "origin" }),
        )?
        .ok_or_else(|| missing("push approval missing"))?;
    let second = coordinator.request_runtime_operation(
        &fixture.run_id,
        "item/commandExecution/requestApproval",
        "42",
        &json!({ "command": ["cargo", "test"] }),
    )?;
    assert_eq!(
        fixture
            .store
            .get_run(&fixture.run_id)?
            .ok_or_else(|| missing("run missing"))?
            .status,
        RunStatus::WaitingApproval
    );
    coordinator.resolve(
        &first.id,
        ApprovalResolution::Approve,
        &json!({ "user": true }),
    )?;
    assert_eq!(
        fixture
            .store
            .get_run(&fixture.run_id)?
            .ok_or_else(|| missing("run missing"))?
            .status,
        RunStatus::WaitingApproval
    );
    coordinator.resolve(
        &second.id,
        ApprovalResolution::Approve,
        &json!({ "user": true }),
    )?;
    assert_eq!(
        fixture
            .store
            .get_run(&fixture.run_id)?
            .ok_or_else(|| missing("run missing"))?
            .status,
        RunStatus::Running
    );
    assert!(
        coordinator
            .resolve(&second.id, ApprovalResolution::Approve, &json!({}))
            .is_err()
    );
    Ok(())
}

#[test]
fn rejection_ends_run_and_cancels_sibling_approvals() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = store_fixture(RunStatus::Running)?;
    let coordinator = ApprovalCoordinator::new(fixture.store.clone());
    let rejected = coordinator
        .request_git_operation(&fixture.run_id, GitOperation::Deploy, &json!({}))?
        .ok_or_else(|| missing("deploy approval missing"))?;
    let sibling = coordinator
        .request_git_operation(&fixture.run_id, GitOperation::Push, &json!({}))?
        .ok_or_else(|| missing("push approval missing"))?;
    coordinator.resolve(&rejected.id, ApprovalResolution::Reject, &json!({}))?;
    assert_eq!(
        fixture
            .store
            .get_run(&fixture.run_id)?
            .ok_or_else(|| missing("run missing"))?
            .status,
        RunStatus::Failed
    );
    assert_eq!(
        fixture
            .store
            .get_approval(&sibling.id)?
            .ok_or_else(|| missing("approval missing"))?
            .status,
        ApprovalStatus::Cancelled
    );
    Ok(())
}

#[tokio::test]
async fn gate_runner_executes_argv_without_a_shell() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let definition = GateDefinition {
        name: "test-harness-list".to_owned(),
        program: std::env::current_exe()?.to_string_lossy().into_owned(),
        args: vec!["--list".to_owned()],
        timeout_ms: 10_000,
    };
    let execution = GateRunner.run(directory.path(), &definition).await?;
    assert_eq!(execution.status, GateStatus::Passed);
    assert!(!execution.timed_out);
    Ok(())
}

#[test]
fn latest_required_gate_result_controls_commit_eligibility() {
    let task_id = EntityId::new();
    let timestamp = now();
    let gate = |name: &str, status| Gate {
        id: EntityId::new(),
        task_id: task_id.clone(),
        run_id: None,
        name: name.to_owned(),
        status,
        command: None,
        result: None,
        created_at: timestamp,
        completed_at: None,
    };
    let gates = [
        gate("format", GateStatus::Passed),
        gate("test", GateStatus::Failed),
    ];
    assert_eq!(
        evaluate_required_gates(&gates, &["format".to_owned(), "test".to_owned()]),
        GateEligibility::Failed(vec!["test".to_owned()])
    );
    assert_eq!(
        evaluate_required_gates(&gates, &["lint".to_owned()]),
        GateEligibility::Missing(vec!["lint".to_owned()])
    );
}

#[test]
fn recovery_orphans_missing_process_and_preserves_resume_path()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = store_fixture(RunStatus::Running)?;
    let stamp = now();
    let dispatch = Dispatch {
        id: EntityId::new(),
        task_id: fixture.task_id.clone(),
        run_id: fixture.run_id.clone(),
        agent_profile_id: None,
        status: "running".into(),
        created_at: stamp,
        updated_at: stamp,
    };
    fixture.store.upsert_dispatch(&dispatch)?;
    let coordinator = ApprovalCoordinator::new(fixture.store.clone());
    let approval = coordinator
        .request_git_operation(&fixture.run_id, GitOperation::Push, &json!({}))?
        .ok_or_else(|| missing("approval missing"))?;
    let recovery = RecoveryCoordinator::new(fixture.store.clone());
    let records = recovery.reconcile(&BTreeSet::new(), &capabilities(true))?;
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].action,
        RecoveryAction::Resume {
            thread_id: "thread-test".to_owned()
        }
    );
    assert_eq!(
        fixture
            .store
            .get_run(&fixture.run_id)?
            .ok_or_else(|| missing("run missing"))?
            .status,
        RunStatus::Orphaned
    );
    assert_eq!(
        fixture
            .store
            .get_task(&fixture.task_id)?
            .ok_or_else(|| missing("task missing"))?
            .status,
        TaskStatus::Blocked
    );
    assert_eq!(
        fixture
            .store
            .get_approval(&approval.id)?
            .ok_or_else(|| missing("approval missing"))?
            .status,
        ApprovalStatus::Cancelled
    );
    assert_eq!(fixture.store.list_run_events(&fixture.run_id, 0)?.len(), 1);
    assert_eq!(
        fixture.store.list_dispatches(&fixture.run_id)?[0].status,
        "orphaned"
    );
    let records = recovery.reconcile(
        &BTreeSet::from([fixture.run_id.clone()]),
        &capabilities(true),
    )?;
    assert_eq!(records[0].action, RecoveryAction::Live);
    assert_eq!(
        fixture.store.list_dispatches(&fixture.run_id)?[0].status,
        "running"
    );
    let _project_exists = fixture.store.get_project(&fixture.project_id)?.is_some();
    Ok(())
}

fn capabilities(resume: bool) -> RuntimeCapabilities {
    RuntimeCapabilities {
        adapter: "codex".to_owned(),
        available: true,
        version: Some(CodexVersion::new(0, 130, 0)),
        minimum_version: MINIMUM_CODEX_VERSION,
        app_server: true,
        exec_json_fallback: true,
        approvals: true,
        resume,
        guide: true,
        cancellation: true,
        degraded_reason: None,
    }
}
