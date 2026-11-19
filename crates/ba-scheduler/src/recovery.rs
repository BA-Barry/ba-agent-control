use std::collections::BTreeSet;

use ba_core::{ApprovalStatus, EntityId, Run, RunStatus, TaskStatus, now};
use ba_runtime::RuntimeCapabilities;
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::SchedulerError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoveryAction {
    Live,
    Resume { thread_id: String },
    RetryTask,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub run_id: EntityId,
    pub previous_status: RunStatus,
    pub action: RecoveryAction,
}

#[derive(Clone, Debug)]
pub struct RecoveryCoordinator {
    store: SqliteStore,
}

impl RecoveryCoordinator {
    #[must_use]
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Reconcile durable active runs with the adapter's live session set.
    ///
    /// Missing processes become orphaned, their obsolete approvals are cancelled,
    /// and their tasks become blocked until an explicit resume or retry action.
    ///
    /// # Errors
    ///
    /// Returns an error when active state cannot be read or reconciliation updates
    /// cannot be persisted.
    pub fn reconcile(
        &self,
        live_runs: &BTreeSet<EntityId>,
        capabilities: &RuntimeCapabilities,
    ) -> Result<Vec<RecoveryRecord>, SchedulerError> {
        let mut active = Vec::new();
        for status in [
            RunStatus::Starting,
            RunStatus::Running,
            RunStatus::WaitingApproval,
            RunStatus::Orphaned,
        ] {
            active.extend(self.store.list_runs_by_status(status)?);
        }
        active.sort_by(|left, right| left.id.cmp(&right.id));

        let mut records = Vec::with_capacity(active.len());
        for run in active {
            let previous_status = run.status;
            if live_runs.contains(&run.id) {
                if run.status == RunStatus::Orphaned {
                    self.store.transition_run(&run.id, RunStatus::Running)?;
                    self.store.update_dispatch_status(&run.id, "running")?;
                    if let Some(task) = self.store.get_task(&run.task_id)?
                        && task.status == TaskStatus::Blocked
                    {
                        self.store.transition_task(&task.id, TaskStatus::Running)?;
                    }
                }
                records.push(RecoveryRecord {
                    run_id: run.id,
                    previous_status,
                    action: RecoveryAction::Live,
                });
                continue;
            }

            if run.status != RunStatus::Orphaned {
                self.store.transition_run(&run.id, RunStatus::Orphaned)?;
            }
            self.store.update_dispatch_status(&run.id, "orphaned")?;
            self.cancel_pending_approvals(&run)?;
            if let Some(task) = self.store.get_task(&run.task_id)?
                && matches!(
                    task.status,
                    TaskStatus::Ready
                        | TaskStatus::Queued
                        | TaskStatus::Running
                        | TaskStatus::Review
                )
            {
                self.store.transition_task(&task.id, TaskStatus::Blocked)?;
            }
            let action = recovery_action(&run, capabilities);
            if previous_status != RunStatus::Orphaned {
                self.store.append_run_event(
                    &run.id,
                    "recovery.orphaned",
                    json!({
                        "previous_status": previous_status,
                        "action": action,
                    }),
                )?;
            }
            records.push(RecoveryRecord {
                run_id: run.id,
                previous_status,
                action,
            });
        }
        Ok(records)
    }

    fn cancel_pending_approvals(&self, run: &Run) -> Result<(), SchedulerError> {
        for mut approval in self.store.list_approvals(&run.id)? {
            if approval.status == ApprovalStatus::Pending {
                approval.status = ApprovalStatus::Cancelled;
                approval.decision = Some(json!({
                    "resolution": "cancelled",
                    "reason": "runtime process was not present during recovery"
                }));
                approval.decided_at = Some(now());
                self.store.upsert_approval(&approval)?;
            }
        }
        Ok(())
    }
}

fn recovery_action(run: &Run, capabilities: &RuntimeCapabilities) -> RecoveryAction {
    if capabilities.available
        && capabilities.resume
        && let Some(thread_id) = run
            .runtime_thread_id
            .as_ref()
            .filter(|thread_id| !thread_id.trim().is_empty())
    {
        return RecoveryAction::Resume {
            thread_id: thread_id.clone(),
        };
    }
    RecoveryAction::RetryTask
}
