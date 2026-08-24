use std::{fs, path::Path};

use ba_core::{
    Artifact, Dispatch, EntityId, Project, ReceiptChainError, Run, RunStatus, Task, TaskStatus, now,
};
use serde_json::json;
use tempfile::TempDir;

use crate::{SqliteStore, StoreError};

fn project() -> Project {
    let time = now();
    Project {
        id: EntityId::new(),
        name: "BA Agent".to_owned(),
        root_path: "C:/repo".to_owned(),
        description: String::new(),
        language: "zh-CN".to_owned(),
        created_at: time,
        updated_at: time,
    }
}

fn task(project_id: EntityId) -> Task {
    let time = now();
    Task {
        id: EntityId::new(),
        project_id,
        workstream_id: None,
        milestone_id: None,
        title: "Implement persistence".to_owned(),
        description: String::new(),
        status: TaskStatus::Draft,
        write: true,
        path_scope: vec!["crates/ba-store".to_owned()],
        depends_on: Vec::new(),
        created_at: time,
        updated_at: time,
    }
}

fn run(task_id: EntityId) -> Run {
    let time = now();
    Run {
        id: EntityId::new(),
        task_id,
        agent_profile_id: None,
        status: RunStatus::Queued,
        adapter: "codex-exec".to_owned(),
        runtime_run_id: None,
        runtime_thread_id: None,
        started_at: None,
        finished_at: None,
        created_at: time,
        updated_at: time,
    }
}

fn open_temp() -> Result<(TempDir, SqliteStore), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let store = SqliteStore::open(temp.path().join("ba-agent.sqlite3"))?;
    Ok((temp, store))
}

#[test]
fn migrates_and_round_trips_entities() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    assert_eq!(store.schema_version()?, 1);
    let project = project();
    let task = task(project.id.clone());
    store.upsert_project(&project)?;
    store.upsert_task(&task)?;
    let loaded = store.get_task(&task.id)?.ok_or("task missing")?;
    assert_eq!(loaded.title, task.title);
    assert_eq!(store.list_tasks(&project.id)?.len(), 1);
    Ok(())
}

#[test]
fn state_transitions_are_checked_transactionally() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    let task = task(project.id.clone());
    store.upsert_project(&project)?;
    store.upsert_task(&task)?;
    assert!(
        store
            .transition_task(&task.id, TaskStatus::Running)
            .is_err()
    );
    assert_eq!(
        store.get_task(&task.id)?.ok_or("task missing")?.status,
        TaskStatus::Draft
    );
    assert_eq!(
        store.transition_task(&task.id, TaskStatus::Ready)?.status,
        TaskStatus::Ready
    );
    Ok(())
}

#[test]
fn run_events_are_append_only_and_strictly_ordered() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    let task = task(project.id.clone());
    let run = run(task.id.clone());
    store.upsert_project(&project)?;
    store.upsert_task(&task)?;
    store.upsert_run(&run)?;
    assert_eq!(
        store
            .append_run_event(&run.id, "started", json!({}))?
            .sequence,
        1
    );
    assert_eq!(
        store
            .append_run_event(&run.id, "output", json!({"text":"ok"}))?
            .sequence,
        2
    );
    let events = store.list_run_events(&run.id, 0)?;
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    Ok(())
}

#[test]
fn terminal_run_update_is_atomic_across_task_dispatch_and_event()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    let mut task = task(project.id.clone());
    task.status = TaskStatus::Running;
    let mut run = run(task.id.clone());
    run.status = RunStatus::Running;
    let stamp = now();
    let dispatch = Dispatch {
        id: EntityId::new(),
        task_id: task.id.clone(),
        run_id: run.id.clone(),
        agent_profile_id: None,
        status: "running".into(),
        created_at: stamp,
        updated_at: stamp,
    };
    store.upsert_project(&project)?;
    store.upsert_task(&task)?;
    store.upsert_run(&run)?;
    store.upsert_dispatch(&dispatch)?;

    let update = store.finish_run(
        &run.id,
        RunStatus::Succeeded,
        "runtime.succeeded",
        json!({"evidence": true}),
    )?;
    assert_eq!(update.run.status, RunStatus::Succeeded);
    assert_eq!(update.task.status, TaskStatus::Review);
    assert_eq!(update.dispatches[0].status, "succeeded");
    assert_eq!(update.event.sequence, 1);
    assert_eq!(store.list_dispatches(&run.id)?[0].status, "succeeded");
    Ok(())
}

#[test]
fn terminal_run_update_rolls_back_when_task_transition_is_invalid()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    let task = task(project.id.clone());
    let mut run = run(task.id.clone());
    run.status = RunStatus::Running;
    store.upsert_project(&project)?;
    store.upsert_task(&task)?;
    store.upsert_run(&run)?;

    assert!(
        store
            .finish_run(
                &run.id,
                RunStatus::Succeeded,
                "runtime.succeeded",
                json!({}),
            )
            .is_err()
    );
    assert_eq!(
        store.get_run(&run.id)?.ok_or("run missing")?.status,
        RunStatus::Running
    );
    assert_eq!(
        store.get_task(&task.id)?.ok_or("task missing")?.status,
        TaskStatus::Draft
    );
    assert!(store.list_run_events(&run.id, 0)?.is_empty());
    Ok(())
}

#[test]
fn artifact_and_receipt_are_recorded_atomically() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    store.upsert_project(&project)?;
    let artifact = Artifact {
        id: EntityId::new(),
        project_id: project.id.clone(),
        task_id: None,
        run_id: None,
        kind: "runtime_state_import".into(),
        path: ".ba-agent/imports/runtime-state/claude/state.json".into(),
        digest: Some("abc123".into()),
        created_at: now(),
    };
    let receipt = store.record_artifact_with_receipt(
        &artifact,
        "runtime_state_import",
        json!({"artifactId": artifact.id}),
    )?;
    assert_eq!(
        store.get_artifact(&artifact.id)?.map(|item| item.id),
        Some(artifact.id.clone())
    );
    let receipts = store.list_receipts(&project.id)?;
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].id, receipt.id);
    store.verify_receipt_chain(&project.id)?;

    assert!(
        store
            .record_artifact_with_receipt(&artifact, "duplicate", json!({}))
            .is_err()
    );
    assert_eq!(store.list_receipts(&project.id)?.len(), 1);
    Ok(())
}

#[test]
fn receipt_chain_detects_tampering() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    store.upsert_project(&project)?;
    let first = store.append_receipt(&project.id, None, None, "gate", json!({"passed":true}))?;
    let second =
        store.append_receipt(&project.id, None, None, "review", json!({"approved":true}))?;
    assert_eq!(second.previous_hash.as_deref(), Some(first.hash.as_str()));
    store.verify_receipt_chain(&project.id)?;

    let connection = rusqlite::Connection::open(store.path())?;
    let mut altered = second.clone();
    altered.payload = json!({"approved":false});
    connection.execute(
        "UPDATE receipts SET data=?1 WHERE id=?2",
        rusqlite::params![serde_json::to_string(&altered)?, second.id.as_str()],
    )?;
    assert!(matches!(
        store.verify_receipt_chain(&project.id),
        Err(StoreError::ReceiptChain(
            ReceiptChainError::HashMismatch { .. }
        ))
    ));
    Ok(())
}

#[test]
fn receipt_chain_detects_normalized_column_tampering() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    store.upsert_project(&project)?;
    let receipt = store.append_receipt(&project.id, None, None, "gate", json!({"passed":true}))?;

    let connection = rusqlite::Connection::open(store.path())?;
    connection.execute(
        "UPDATE receipts SET hash=?1 WHERE id=?2",
        rusqlite::params!["0".repeat(64), receipt.id.as_str()],
    )?;

    assert!(matches!(
        store.verify_receipt_chain(&project.id),
        Err(StoreError::ReceiptChain(
            ReceiptChainError::StoredFieldMismatch { field: "hash", .. }
        ))
    ));
    Ok(())
}

#[test]
fn receipt_chain_requires_contiguous_sequence_numbers() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    store.upsert_project(&project)?;
    let receipt = store.append_receipt(&project.id, None, None, "gate", json!({"passed":true}))?;

    let connection = rusqlite::Connection::open(store.path())?;
    connection.execute(
        "UPDATE receipts SET chain_sequence=2 WHERE id=?1",
        rusqlite::params![receipt.id.as_str()],
    )?;

    assert!(matches!(
        store.verify_receipt_chain(&project.id),
        Err(StoreError::ReceiptChain(ReceiptChainError::Sequence {
            expected: 1,
            actual: 2,
            ..
        }))
    ));
    Ok(())
}

#[test]
fn retention_does_not_delete_receipts() -> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store) = open_temp()?;
    let project = project();
    let task = task(project.id.clone());
    let run = run(task.id.clone());
    store.upsert_project(&project)?;
    store.upsert_task(&task)?;
    store.upsert_run(&run)?;
    store.append_run_event(&run.id, "old", json!({}))?;
    store.append_runtime_log(Some(&project.id), Some(&run.id), "info", "old")?;
    store.append_receipt(
        &project.id,
        Some(task.id),
        Some(run.id),
        "permanent",
        json!({}),
    )?;
    let report = store.prune_runtime_data(now() + chrono::Duration::seconds(1))?;
    assert_eq!(report.events, 1);
    assert_eq!(report.logs, 1);
    assert_eq!(store.list_receipts(&project.id)?.len(), 1);
    Ok(())
}

#[test]
fn creates_pre_migration_backup() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("legacy.sqlite3");
    {
        let connection = rusqlite::Connection::open(&path)?;
        connection.execute("CREATE TABLE legacy(value TEXT)", [])?;
        connection.execute("INSERT INTO legacy(value) VALUES ('preserve me')", [])?;
    }
    let store = SqliteStore::open(&path)?;
    let backup = store.last_backup().ok_or("backup was not created")?;
    assert!(backup.exists());
    assert!(backup_contains_legacy_data(backup)?);
    Ok(())
}

fn backup_contains_legacy_data(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let connection = rusqlite::Connection::open(path)?;
    let value: String = connection.query_row("SELECT value FROM legacy", [], |row| row.get(0))?;
    Ok(value == "preserve me" && fs::metadata(path)?.len() > 0)
}
