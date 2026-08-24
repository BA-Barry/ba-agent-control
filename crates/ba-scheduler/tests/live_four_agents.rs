use std::{collections::BTreeSet, sync::Arc, time::Duration};

use ba_core::{EntityId, Project, Task, TaskStatus, now};
use ba_runtime::CodexAdapter;
use ba_scheduler::{DispatchCoordinator, RuntimeCoordinator, ensure_builtin_agent_profiles};
use ba_store::SqliteStore;

#[tokio::test]
#[ignore = "requires a logged-in Codex CLI and performs four live model turns"]
async fn four_profiles_run_concurrently_and_the_fifth_is_blocked()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let store = SqliteStore::open(directory.path().join("state.sqlite3"))?;
    let project = Project {
        id: EntityId::new(),
        name: "Four-agent live demonstration".into(),
        root_path: directory
            .path()
            .canonicalize()?
            .to_string_lossy()
            .into_owned(),
        description: String::new(),
        language: "en".into(),
        created_at: now(),
        updated_at: now(),
    };
    store.upsert_project(&project)?;
    let profiles = ensure_builtin_agent_profiles(&store)?;
    let selected_roles = ["coordinator", "planner", "explorer", "reviewer"];
    let selected = selected_roles
        .iter()
        .map(|role| {
            profiles
                .iter()
                .find(|profile| profile.role == *role)
                .map(|profile| profile.id.clone())
                .ok_or_else(|| format!("profile missing: {role}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let worktree_root = directory.path().join("worktrees");
    let coordinator = RuntimeCoordinator::new(store.clone(), Arc::new(CodexAdapter::default()));
    let mut events = coordinator.subscribe_events();

    let mut launches = Vec::new();
    let mut run_ids = BTreeSet::new();
    let mut task_ids = Vec::new();
    for (index, profile_id) in selected.iter().enumerate() {
        let task = read_task(&project.id, &format!("parallel-agent-{}", index + 1));
        store.upsert_task(&task)?;
        task_ids.push(task.id.clone());
        let bundle = DispatchCoordinator::new(store.clone(), worktree_root.clone())
            .enqueue(&task.id, Some(profile_id))?;
        run_ids.insert(bundle.run.id.clone());
        let coordinator = coordinator.clone();
        launches.push(tokio::spawn(async move {
            coordinator
                .launch(
                    bundle,
                    Some("Respond with exactly BA_AGENT_PARALLEL_OK. Do not use tools.".into()),
                )
                .await
        }));
    }
    for launch in launches {
        launch.await??;
    }

    let fifth = read_task(&project.id, "capacity-probe");
    store.upsert_task(&fifth)?;
    assert!(
        DispatchCoordinator::new(store.clone(), worktree_root)
            .enqueue(&fifth.id, None)
            .is_err(),
        "the fifth task bypassed the four-run concurrency limit"
    );

    let completed = tokio::time::timeout(Duration::from_secs(240), async {
        let mut completed = BTreeSet::new();
        while completed.len() < 4 {
            let event = events.recv().await?;
            if run_ids.contains(&event.run_id) && event.kind == "runtime.succeeded" {
                completed.insert(event.run_id);
            }
        }
        Ok::<_, tokio::sync::broadcast::error::RecvError>(completed)
    })
    .await;
    if completed.is_err() {
        for run_id in &run_ids {
            let _ignored = coordinator.cancel(run_id, "live_test_timeout").await;
        }
    }
    assert_eq!(completed??.len(), 4);
    for task_id in task_ids {
        assert_eq!(
            store
                .get_task(&task_id)?
                .ok_or("parallel task missing")?
                .status,
            TaskStatus::Review
        );
    }
    Ok(())
}

fn read_task(project_id: &EntityId, title: &str) -> Task {
    let stamp = now();
    Task {
        id: EntityId::new(),
        project_id: project_id.clone(),
        workstream_id: None,
        milestone_id: None,
        title: title.into(),
        description: String::new(),
        status: TaskStatus::Draft,
        write: false,
        path_scope: Vec::new(),
        depends_on: Vec::new(),
        created_at: stamp,
        updated_at: stamp,
    }
}
