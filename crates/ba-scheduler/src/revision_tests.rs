use super::*;
use crate::{
    DispatchBundle, ReviewRepairAction, ReviewRepairLoopDecisionRequest, ReviewRepairLoopService,
    ReviewRepairLoopStartRequest, task_execution_directory,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn repository_fixture() -> TestResult<StoreFixture> {
    let fixture = store_fixture(RunStatus::Succeeded)?;
    let root = fixture.directory.path();
    run_git(root, &["init", "-b", "main"])?;
    run_git(root, &["config", "user.email", "test@example.invalid"])?;
    run_git(root, &["config", "user.name", "BA Agent Test"])?;
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join(".ba-agent"))?;
    std::fs::write(root.join("src/base.txt"), "base\n")?;
    std::fs::write(root.join(".gitignore"), "state.sqlite3\nstate.sqlite3-*\n")?;
    std::fs::write(
        root.join(".ba-agent/project.toml"),
        "[gates]\ncheck = [\"git\", \"diff\", \"--check\"]\n",
    )?;
    run_git(root, &["add", ".gitignore", ".ba-agent", "src/base.txt"])?;
    run_git(root, &["commit", "-m", "base"])?;
    ensure_builtin_agent_profiles(&fixture.store)?;
    Ok(fixture)
}

fn stage(store: &SqliteStore, review_id: &EntityId) -> TestResult<Vec<Task>> {
    let review = store
        .get_task(review_id)?
        .ok_or_else(|| missing("review missing"))?;
    let test = store
        .get_task(&review.depends_on[0])?
        .ok_or_else(|| missing("test missing"))?;
    let writer = store
        .get_task(&test.depends_on[0])?
        .ok_or_else(|| missing("writer missing"))?;
    Ok(vec![writer, test, review])
}

async fn launch(store: &SqliteStore, worktrees: &Path, task: &Task) -> TestResult<DispatchBundle> {
    let dispatcher = DispatchCoordinator::new(store.clone(), worktrees.to_path_buf());
    let runtime = Arc::new(HoldingRuntime::new());
    let control =
        RuntimeCoordinator::new(store.clone(), runtime.clone()).with_queue(worktrees.to_path_buf());
    let bundle = control
        .launch(dispatcher.enqueue(&task.id, None)?, None)
        .await?;
    let checkout = bundle
        .worktree
        .as_ref()
        .ok_or_else(|| missing("snapshot missing"))?;
    let launches = runtime
        .launches
        .lock()
        .map_err(|_| missing("launch log poisoned"))?;
    assert_eq!(
        launches[0].1.cwd,
        PathBuf::from(&checkout.path).canonicalize()?
    );
    if !task.write {
        assert_eq!(launches[0].1.sandbox, ba_runtime::SandboxMode::ReadOnly);
        assert!(checkout.write_scope.is_empty());
    }
    Ok(bundle)
}

async fn finish(
    store: &SqliteStore,
    project_root: &Path,
    worktrees: &Path,
    bundle: &DispatchBundle,
) -> TestResult<crate::FinalizationResult> {
    store.finish_run(
        &bundle.run.id,
        RunStatus::Succeeded,
        "test.runtime.succeeded",
        json!({}),
    )?;
    let task = store
        .get_task(&bundle.run.task_id)?
        .ok_or_else(|| missing("task missing"))?;
    let cwd = task_execution_directory(store, &task, Some(&bundle.run.id), project_root)?;
    assert_eq!(
        cwd,
        PathBuf::from(
            &bundle
                .worktree
                .as_ref()
                .ok_or_else(|| missing("snapshot missing"))?
                .path
        )
        .canonicalize()?
    );
    let definition = configured_gate(project_root, "check")?;
    let gate = GateCoordinator::new(store.clone())
        .run(&task.id, Some(&bundle.run.id), &cwd, &definition)
        .await?;
    assert_eq!(gate.status, GateStatus::Passed);
    FinalizationCoordinator::new(store.clone(), worktrees.to_path_buf())
        .finalize_if_eligible(&task.id, Some(&bundle.run.id))?
        .ok_or_else(|| missing("not finalized").into())
}

async fn execute_stage(
    fixture: &StoreFixture,
    worktrees: &Path,
    review_id: &EntityId,
    round: usize,
    previous_commit: &mut Option<String>,
) -> TestResult {
    let tasks = stage(&fixture.store, review_id)?;
    for (index, task) in tasks.iter().enumerate() {
        let bundle = launch(&fixture.store, worktrees, task).await?;
        let snapshot = bundle
            .worktree
            .as_ref()
            .ok_or_else(|| missing("snapshot missing"))?;
        if let Some(commit) = previous_commit.as_ref() {
            assert_eq!(snapshot.base_revision.as_ref(), Some(commit));
            assert!(Path::new(&snapshot.path).join("src/feature.txt").is_file());
        }
        if round == 1 {
            assert!(Path::new(&snapshot.path).join("src/tests-0.txt").is_file());
        }
        match index {
            0 => std::fs::write(
                Path::new(&snapshot.path).join("src/feature.txt"),
                format!("implementation {round}\n"),
            )?,
            1 => {
                assert_eq!(
                    std::fs::read_to_string(Path::new(&snapshot.path).join("src/feature.txt"))?
                        .trim_end(),
                    format!("implementation {round}")
                );
                std::fs::write(
                    Path::new(&snapshot.path).join(format!("src/tests-{round}.txt")),
                    "tested\n",
                )?;
            }
            _ => assert!(
                Path::new(&snapshot.path)
                    .join(format!("src/tests-{round}.txt"))
                    .is_file()
            ),
        }
        let finalized =
            finish(&fixture.store, fixture.directory.path(), worktrees, &bundle).await?;
        if let Some(git) = finalized.git {
            *previous_commit = Some(git.commit);
        } else {
            assert_eq!(
                finalized.receipt.payload["commit"].as_str(),
                previous_commit.as_deref()
            );
        }
        assert!(!fixture.directory.path().join("src/feature.txt").exists());
    }
    Ok(())
}

#[tokio::test]
async fn repair_loop_reads_actual_upstream_commits_through_confirmation() -> TestResult {
    let fixture = repository_fixture()?;
    let worktrees = tempfile::tempdir()?;
    let service = ReviewRepairLoopService::new(fixture.store.clone());
    let mut workflow = service.start(&ReviewRepairLoopStartRequest {
        project_id: fixture.project_id.clone(),
        title: "Version propagation".into(),
        objective: "Implement, test, review and repair the same code".into(),
        write_scope: vec!["src".into()],
        max_repairs: 1,
    })?;
    let mut previous_commit = None;
    for round in 0..2 {
        execute_stage(
            &fixture,
            worktrees.path(),
            &workflow.active_review_task_id,
            round,
            &mut previous_commit,
        )
        .await?;
        workflow = service.decide(
            &workflow.id,
            &ReviewRepairLoopDecisionRequest {
                action: if round == 0 {
                    ReviewRepairAction::Repair
                } else {
                    ReviewRepairAction::Accept
                },
                note: "Explicit test-user decision after inspecting the selected code".into(),
            },
        )?;
    }
    // Reopen SQLite before confirmation to prove the selection is durable.
    let reopened = SqliteStore::open(fixture.directory.path().join("state.sqlite3"))?;
    let confirmation = reopened
        .get_task(
            workflow
                .confirmation_task_id
                .as_ref()
                .ok_or_else(|| missing("confirmation missing"))?,
        )?
        .ok_or_else(|| missing("task missing"))?;
    let bundle = launch(&reopened, worktrees.path(), &confirmation).await?;
    assert_eq!(
        bundle
            .worktree
            .as_ref()
            .and_then(|item| item.base_revision.as_ref()),
        previous_commit.as_ref()
    );
    finish(
        &reopened,
        fixture.directory.path(),
        worktrees.path(),
        &bundle,
    )
    .await?;
    assert_eq!(
        ReviewRepairLoopService::new(reopened.clone())
            .get(&workflow.id)?
            .status,
        ba_core::ReviewRepairLoopStatus::Completed
    );
    fixture.store.verify_receipt_chain(&fixture.project_id)?;
    Ok(())
}

#[tokio::test]
async fn divergent_or_unproven_write_inputs_do_not_fall_back_to_checkout() -> TestResult {
    let fixture = repository_fixture()?;
    let worktrees = tempfile::tempdir()?;
    let mut upstreams = Vec::new();
    for index in 0..2 {
        let writer = task(&fixture.project_id, "independent change", true, &["src"]);
        fixture.store.upsert_task(&writer)?;
        let bundle = launch(&fixture.store, worktrees.path(), &writer).await?;
        std::fs::write(
            Path::new(
                &bundle
                    .worktree
                    .as_ref()
                    .ok_or_else(|| missing("snapshot missing"))?
                    .path,
            )
            .join(format!("src/change-{index}.txt")),
            "change\n",
        )?;
        finish(
            &fixture.store,
            fixture.directory.path(),
            worktrees.path(),
            &bundle,
        )
        .await?;
        upstreams.push(writer.id);
    }
    let mut reader = task(&fixture.project_id, "review both", false, &[]);
    reader.depends_on = upstreams;
    let dispatcher =
        DispatchCoordinator::new(fixture.store.clone(), worktrees.path().to_path_buf());
    let result = dispatcher.create_worktree(&reader, &EntityId::new());
    assert!(
        matches!(result, Err(SchedulerError::NotSchedulable(message)) if message.contains("diverge"))
    );
    let mut unproven = task(&fixture.project_id, "unproven", true, &["src"]);
    unproven.status = TaskStatus::Completed;
    fixture.store.upsert_task(&unproven)?;
    reader.depends_on = vec![unproven.id];
    assert!(
        matches!(dispatcher.create_worktree(&reader, &EntityId::new()), Err(SchedulerError::NotSchedulable(message)) if message.contains("no finalized commit"))
    );
    Ok(())
}
