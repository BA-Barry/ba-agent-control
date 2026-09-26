use ba_core::{Approval, ApprovalStatus, EntityId, RunStatus, now};
use ba_git::GitOperation;
use ba_store::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::SchedulerError;

/// Whether this request supports session reuse within BA Agent's policy.
/// MCP elicitation has no session decision; permission elevation stays per-call.
#[must_use]
pub fn approval_supports_session(approval: &Approval) -> bool {
    if !matches!(
        approval.kind.as_str(),
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
    ) || approval
        .request
        .get("request_id")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return false;
    }
    let payload = &approval.request["payload"];
    let params = payload.get("params").unwrap_or(payload);
    match params.get("availableDecisions") {
        None | Some(Value::Null) => true,
        Some(Value::Array(decisions)) => decisions.iter().any(|value| value == "acceptForSession"),
        Some(_) => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalResolution {
    Approve,
    Reject,
    Cancel,
}

impl ApprovalResolution {
    const fn status(self) -> ApprovalStatus {
        match self {
            Self::Approve => ApprovalStatus::Approved,
            Self::Reject => ApprovalStatus::Rejected,
            Self::Cancel => ApprovalStatus::Cancelled,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApprovalCoordinator {
    store: SqliteStore,
}

impl ApprovalCoordinator {
    #[must_use]
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Persist an approval boundary for protected Git/deployment operations.
    /// Local branch and commit operations return `None` and remain automatic.
    ///
    /// # Errors
    ///
    /// Returns an error if the run is missing/inactive or durable state cannot be
    /// updated.
    pub fn request_git_operation(
        &self,
        run_id: &EntityId,
        operation: GitOperation,
        details: &Value,
    ) -> Result<Option<Approval>, SchedulerError> {
        if !operation.requires_approval() {
            return Ok(None);
        }
        let run = self
            .store
            .get_run(run_id)?
            .ok_or_else(|| SchedulerError::ApprovalRunInactive(run_id.to_string()))?;
        if !matches!(run.status, RunStatus::Running | RunStatus::WaitingApproval) {
            return Err(SchedulerError::ApprovalRunInactive(run_id.to_string()));
        }
        let approval = Approval {
            id: EntityId::new(),
            run_id: run_id.clone(),
            kind: "protected_git_operation".to_owned(),
            status: ApprovalStatus::Pending,
            request: json!({ "operation": operation, "details": details }),
            decision: None,
            requested_at: now(),
            decided_at: None,
        };
        self.store.upsert_approval(&approval)?;
        if run.status == RunStatus::Running {
            self.store
                .transition_run(run_id, RunStatus::WaitingApproval)?;
        }
        Ok(Some(approval))
    }

    /// Persist an App Server approval request before exposing it to the UI.
    ///
    /// # Errors
    ///
    /// Returns an error if the run is missing/inactive or durable state cannot be
    /// updated.
    pub fn request_runtime_operation(
        &self,
        run_id: &EntityId,
        method: &str,
        request_id: &str,
        request: &Value,
    ) -> Result<Approval, SchedulerError> {
        let run = self
            .store
            .get_run(run_id)?
            .ok_or_else(|| SchedulerError::ApprovalRunInactive(run_id.to_string()))?;
        if !matches!(run.status, RunStatus::Running | RunStatus::WaitingApproval) {
            return Err(SchedulerError::ApprovalRunInactive(run_id.to_string()));
        }
        let approval = Approval {
            id: EntityId::new(),
            run_id: run_id.clone(),
            kind: method.to_owned(),
            status: ApprovalStatus::Pending,
            request: json!({ "request_id": request_id, "payload": request }),
            decision: None,
            requested_at: now(),
            decided_at: None,
        };
        self.store.upsert_approval(&approval)?;
        if run.status == RunStatus::Running {
            self.store
                .transition_run(run_id, RunStatus::WaitingApproval)?;
        }
        Ok(approval)
    }

    /// Resolve a protected policy approval and advance or terminate its run consistently.
    ///
    /// # Errors
    ///
    /// Returns an error if the approval is missing/already resolved, its run is no
    /// longer active, or durable state cannot be updated.
    pub fn resolve(
        &self,
        approval_id: &EntityId,
        resolution: ApprovalResolution,
        evidence: &Value,
    ) -> Result<Approval, SchedulerError> {
        self.resolve_with_policy(approval_id, resolution, evidence, true, false)
    }

    /// Resolve a protected approval while leaving terminal Run synchronization
    /// to [`crate::RuntimeCoordinator`].
    ///
    /// This keeps the approval record and sibling cancellation in one policy
    /// boundary while allowing the runtime coordinator to atomically finish the
    /// Run, Task, Dispatch, and terminal event.
    ///
    /// # Errors
    ///
    /// Returns an error if the approval is missing/already resolved, its Run is no
    /// longer active, or durable state cannot be updated.
    pub(crate) fn resolve_managed(
        &self,
        approval_id: &EntityId,
        resolution: ApprovalResolution,
        evidence: &Value,
    ) -> Result<Approval, SchedulerError> {
        self.resolve_with_policy(approval_id, resolution, evidence, true, true)
    }

    /// Resolve a runtime approval without prematurely deciding the Run outcome.
    ///
    /// A declined tool call is delivered back to the runtime, which may choose a
    /// safe alternative and still complete the Task. Protected Git approvals use
    /// [`Self::resolve`] instead because their rejection is a firm boundary.
    ///
    /// # Errors
    ///
    /// Returns an error if the approval is missing/already resolved, its Run is no
    /// longer active, or durable state cannot be updated.
    pub fn resolve_runtime(
        &self,
        approval_id: &EntityId,
        resolution: ApprovalResolution,
        evidence: &Value,
    ) -> Result<Approval, SchedulerError> {
        self.resolve_with_policy(approval_id, resolution, evidence, false, false)
    }

    fn resolve_with_policy(
        &self,
        approval_id: &EntityId,
        resolution: ApprovalResolution,
        evidence: &Value,
        terminate_on_rejection: bool,
        defer_terminal_transition: bool,
    ) -> Result<Approval, SchedulerError> {
        let mut approval = self
            .store
            .get_approval(approval_id)?
            .ok_or_else(|| SchedulerError::ApprovalAlreadyResolved(approval_id.to_string()))?;
        if approval.status != ApprovalStatus::Pending {
            return Err(SchedulerError::ApprovalAlreadyResolved(
                approval_id.to_string(),
            ));
        }
        approval.status = resolution.status();
        approval.decision = Some(json!({
            "resolution": resolution,
            "evidence": evidence,
        }));
        approval.decided_at = Some(now());
        if !self.store.update_approval_if_pending(&approval)? {
            return Err(SchedulerError::ApprovalAlreadyResolved(
                approval_id.to_string(),
            ));
        }

        if terminate_on_rejection
            && matches!(
                resolution,
                ApprovalResolution::Reject | ApprovalResolution::Cancel
            )
        {
            self.cancel_sibling_approvals(&approval)?;
            if !defer_terminal_transition {
                let run = self.store.get_run(&approval.run_id)?.ok_or_else(|| {
                    SchedulerError::ApprovalRunInactive(approval.run_id.to_string())
                })?;
                if run.status == RunStatus::WaitingApproval {
                    let terminal = if resolution == ApprovalResolution::Cancel {
                        RunStatus::Cancelled
                    } else {
                        RunStatus::Failed
                    };
                    self.store.transition_run(&approval.run_id, terminal)?;
                }
            }
        } else {
            let any_pending = self
                .store
                .list_approvals(&approval.run_id)?
                .iter()
                .any(|candidate| candidate.status == ApprovalStatus::Pending);
            let run = self
                .store
                .get_run(&approval.run_id)?
                .ok_or_else(|| SchedulerError::ApprovalRunInactive(approval.run_id.to_string()))?;
            if !any_pending && run.status == RunStatus::WaitingApproval {
                self.store
                    .transition_run(&approval.run_id, RunStatus::Running)?;
            }
        }
        Ok(approval)
    }

    fn cancel_sibling_approvals(&self, resolved: &Approval) -> Result<(), SchedulerError> {
        for mut sibling in self.store.list_approvals(&resolved.run_id)? {
            if sibling.id != resolved.id && sibling.status == ApprovalStatus::Pending {
                sibling.status = ApprovalStatus::Cancelled;
                sibling.decision = Some(json!({
                    "resolution": "cancelled",
                    "reason": "another approval ended the run"
                }));
                sibling.decided_at = Some(now());
                self.store.upsert_approval(&sibling)?;
            }
        }
        Ok(())
    }
}
