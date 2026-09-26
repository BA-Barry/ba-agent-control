use std::{error::Error, sync::Barrier, thread};

use ba_store::StoreError;

use super::{
    tests::{complete_task, fixture},
    *,
};

fn start(
    service: &ReviewRepairLoopService,
    project: &Project,
) -> Result<ReviewRepairLoop, SchedulerError> {
    service.start(&ReviewRepairLoopStartRequest {
        project_id: project.id.clone(),
        title: "atomic review decision".into(),
        objective: "retain the explicit acceptance boundary across concurrent decisions and restart".into(),
        write_scope: vec!["crates/ba-scheduler/**".into(), "docs/**".into()],
        max_repairs: 1,
    })
}

fn request(action: ReviewRepairAction) -> ReviewRepairLoopDecisionRequest {
    ReviewRepairLoopDecisionRequest {
        action,
        note: format!("explicit {action:?} decision"),
    }
}

fn race(
    store: &SqliteStore,
    id: &EntityId,
    actions: [ReviewRepairAction; 2],
) -> Result<ReviewRepairLoop, Box<dyn Error>> {
    // Independent connections cover SQLite arbitration, not just the shared
    // SqliteStore mutex. Both requests observe the same review before either
    // can persist its decision; no sleeps or probabilistic scheduling.
    let services = [
        ReviewRepairLoopService::new(SqliteStore::open(store.path())?),
        ReviewRepairLoopService::new(SqliteStore::open(store.path())?),
    ];
    let snapshots = [services[0].get(id)?, services[1].get(id)?];
    let barrier = Barrier::new(2);
    let results = thread::scope(|scope| {
        let workers = services
            .into_iter()
            .zip(snapshots)
            .zip(actions)
            .map(|((service, snapshot), action)| {
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    service.decide_review(snapshot, &request(action))
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| std::io::Error::other("decision worker panicked"))
            })
            .collect::<Result<Vec<_>, _>>()
    })?;
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "{results:?}"
    );
    assert_eq!(results.iter().filter(|result| matches!(result, Err(SchedulerError::Store(StoreError::ConcurrentReviewRepairLoop(conflict_id))) if conflict_id == id)).count(), 1, "{results:?}");
    results
        .into_iter()
        .find_map(Result::ok)
        .ok_or_else(|| std::io::Error::other("no winning decision").into())
}

#[test]
fn concurrent_review_decisions_commit_one_stage_and_recover() -> Result<(), Box<dyn Error>> {
    use ReviewRepairAction::{Accept, Block, Repair};
    for actions in [
        [Repair, Repair],
        [Accept, Repair],
        [Accept, Accept],
        [Block, Repair],
        [Block, Accept],
    ] {
        let (_directory, store, project) = fixture()?;
        let service = ReviewRepairLoopService::new(store.clone());
        let created = start(&service, &project)?;
        complete_task(&store, &created.active_review_task_id)?;
        let winner = race(&store, &created.id, actions)?;
        assert_eq!(winner.decisions.len(), 1);
        assert_eq!(
            winner.decisions[0].review_task_id,
            created.active_review_task_id
        );

        let tasks = store.list_tasks(&project.id)?;
        match winner.status {
            ReviewRepairLoopStatus::Running => {
                assert_eq!(winner.repairs_started, 1);
                assert!(winner.confirmation_task_id.is_none());
                assert_eq!(tasks.len(), 6);
                let [repair, test, review] = service.active_stage(&winner)?;
                assert_eq!(
                    repair.depends_on,
                    vec![created.active_review_task_id.clone()]
                );
                assert_eq!(test.depends_on, vec![repair.id]);
                assert_eq!(review.depends_on, vec![test.id]);
                assert_eq!(repair.path_scope, created.write_scope);
                assert_eq!(test.path_scope, created.write_scope);
            }
            ReviewRepairLoopStatus::Confirming => {
                assert_eq!(winner.repairs_started, 0);
                assert_eq!(tasks.len(), 4);
                let confirmation = tasks
                    .iter()
                    .find(|task| Some(&task.id) == winner.confirmation_task_id.as_ref())
                    .ok_or_else(|| std::io::Error::other("confirmation missing"))?;
                assert_eq!(
                    confirmation.depends_on,
                    vec![created.active_review_task_id.clone()]
                );
                assert!(!confirmation.write);
            }
            ReviewRepairLoopStatus::Blocked => {
                assert_eq!(winner.repairs_started, 0);
                assert!(winner.confirmation_task_id.is_none());
                assert_eq!(tasks.len(), 3);
            }
            status => panic!("unexpected winner status: {status:?}"),
        }
        assert!(
            tasks
                .iter()
                .all(|task| task.description.contains(&created.objective))
        );
        assert!(service.decide(&created.id, &request(Repair)).is_err());
        let path = store.path().to_path_buf();
        drop(service);
        drop(store);
        let reopened = SqliteStore::open(path)?;
        let recovered = ReviewRepairLoopService::new(reopened.clone()).get(&created.id)?;
        assert_eq!(
            serde_json::to_value(&recovered)?,
            serde_json::to_value(&winner)?
        );
        assert_eq!(recovered.objective, created.objective);
        assert_eq!(recovered.write_scope, created.write_scope);
        assert_eq!(reopened.list_tasks(&project.id)?.len(), tasks.len());
    }
    Ok(())
}

#[test]
fn exhausted_repair_and_accept_still_require_one_decision() -> Result<(), Box<dyn Error>> {
    for actions in [
        [ReviewRepairAction::Repair, ReviewRepairAction::Repair],
        [ReviewRepairAction::Repair, ReviewRepairAction::Accept],
    ] {
        let (_directory, store, project) = fixture()?;
        let service = ReviewRepairLoopService::new(store.clone());
        let created = start(&service, &project)?;
        complete_task(&store, &created.active_review_task_id)?;
        let repaired = service.decide(&created.id, &request(ReviewRepairAction::Repair))?;
        complete_task(&store, &repaired.active_review_task_id)?;
        let winner = race(&store, &created.id, actions)?;
        assert_eq!(winner.repairs_started, 1);
        assert_eq!(winner.decisions.len(), 2);
        assert_eq!(
            winner.decisions[1].review_task_id,
            repaired.active_review_task_id
        );
        let expected_tasks = match winner.status {
            ReviewRepairLoopStatus::Exhausted => 6,
            ReviewRepairLoopStatus::Confirming => 7,
            status => panic!("unexpected winner status: {status:?}"),
        };
        assert_eq!(store.list_tasks(&project.id)?.len(), expected_tasks);
    }
    Ok(())
}

#[test]
fn stale_refresh_and_decision_cannot_overwrite_or_decide_a_later_review()
-> Result<(), Box<dyn Error>> {
    let (_directory, store, project) = fixture()?;
    let service = ReviewRepairLoopService::new(store.clone());
    let created = start(&service, &project)?;
    complete_task(&store, &created.active_review_task_id)?;
    let old_awaiting = service.get(&created.id)?;
    let repaired = service.decide(&created.id, &request(ReviewRepairAction::Repair))?;
    // Both snapshots are Running, but refer to different review tasks.
    let refreshed = service.refresh(created.clone())?;
    assert_eq!(
        serde_json::to_value(refreshed)?,
        serde_json::to_value(&repaired)?
    );
    complete_task(&store, &repaired.active_review_task_id)?;
    let awaiting = service.get(&created.id)?;
    assert_eq!(awaiting.status, ReviewRepairLoopStatus::AwaitingDecision);
    for stale in [created, old_awaiting] {
        assert!(matches!(
            service.decide_review(stale, &request(ReviewRepairAction::Accept)),
            Err(SchedulerError::Store(
                StoreError::ConcurrentReviewRepairLoop(_)
            ))
        ));
    }
    let persisted = service.get(&repaired.id)?;
    assert_eq!(
        serde_json::to_value(persisted)?,
        serde_json::to_value(awaiting)?
    );
    assert_eq!(store.list_tasks(&project.id)?.len(), 6);
    Ok(())
}

#[test]
fn stale_running_decisions_report_conflict_when_the_winner_keeps_the_review()
-> Result<(), Box<dyn Error>> {
    let mut unexpected_results = Vec::new();
    for (winning_action, expected_status) in [
        (
            ReviewRepairAction::Accept,
            ReviewRepairLoopStatus::Confirming,
        ),
        (ReviewRepairAction::Block, ReviewRepairLoopStatus::Blocked),
        (
            ReviewRepairAction::Repair,
            ReviewRepairLoopStatus::Exhausted,
        ),
    ] {
        let (_directory, store, project) = fixture()?;
        let service = ReviewRepairLoopService::new(store.clone());
        let created = start(&service, &project)?;
        let active = if winning_action == ReviewRepairAction::Repair {
            complete_task(&store, &created.active_review_task_id)?;
            service.decide(&created.id, &request(ReviewRepairAction::Repair))?
        } else {
            created
        };
        complete_task(&store, &active.active_review_task_id)?;
        let stale = store
            .get_review_repair_loop(&active.id)?
            .ok_or_else(|| std::io::Error::other("loop missing before decision"))?;
        assert_eq!(stale.status, ReviewRepairLoopStatus::Running);
        let winner = service.decide(&active.id, &request(winning_action))?;
        assert_eq!(winner.status, expected_status);
        assert_eq!(winner.active_review_task_id, stale.active_review_task_id);
        let task_count = store.list_tasks(&project.id)?.len();

        let result = service.decide_review(stale, &request(ReviewRepairAction::Accept));
        if !matches!(
            &result,
            Err(SchedulerError::Store(StoreError::ConcurrentReviewRepairLoop(id)))
                if id == &active.id
        ) {
            unexpected_results.push(format!("{winning_action:?}: {result:?}"));
        }
        assert_eq!(
            serde_json::to_value(service.get(&active.id)?)?,
            serde_json::to_value(winner)?
        );
        assert_eq!(store.list_tasks(&project.id)?.len(), task_count);
    }
    assert!(
        unexpected_results.is_empty(),
        "every stale request must report ConcurrentReviewRepairLoop; actual results:\n{}",
        unexpected_results.join("\n")
    );
    Ok(())
}

#[test]
fn failed_task_insert_rolls_back_decision_and_all_generated_tasks() -> Result<(), Box<dyn Error>> {
    let (_directory, store, project) = fixture()?;
    let service = ReviewRepairLoopService::new(store.clone());
    let created = start(&service, &project)?;
    complete_task(&store, &created.active_review_task_id)?;
    let expected = service.get(&created.id)?;
    let mut proposed = expected.clone();
    proposed.decisions.push(ReviewRepairDecision {
        decision: "repair".into(),
        review_task_id: expected.active_review_task_id.clone(),
        note: "repair evidence".into(),
        decided_at: now(),
    });
    let mut tasks = service.request_repair(&mut proposed, "repair evidence", now())?;
    let generated_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
    tasks.push(service.task(&created.active_review_task_id)?); // duplicate primary key after three inserts
    assert!(matches!(
        store.advance_review_repair_loop(&expected, &proposed, &tasks),
        Err(StoreError::Sqlite(_))
    ));
    assert_eq!(
        serde_json::to_value(service.get(&created.id)?)?,
        serde_json::to_value(expected)?
    );
    assert_eq!(store.list_tasks(&project.id)?.len(), 3);
    for id in generated_ids {
        assert!(store.get_task(&id)?.is_none());
    }
    let repaired = service.decide(&created.id, &request(ReviewRepairAction::Repair))?;
    assert_eq!(repaired.decisions.len(), 1);
    assert_eq!(store.list_tasks(&project.id)?.len(), 6);
    Ok(())
}
