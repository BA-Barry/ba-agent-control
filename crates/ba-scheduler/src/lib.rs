//! Dependency-aware scheduling, approvals, quality gates, and restart recovery.
//!
//! The scheduler is deliberately policy-heavy and transport-light: it decides what
//! may run and durably records decisions, while [`ba_runtime`] owns Codex processes
//! and [`ba_git`] owns repository mutations.

mod approval;
mod dispatch;
mod finalize;
mod gate;
mod planner;
mod policy;
mod profiles;
mod recovery;
mod runtime_control;

pub use approval::{ApprovalCoordinator, ApprovalResolution};
pub use dispatch::{DispatchBundle, DispatchCoordinator};
pub use finalize::{FinalizationCoordinator, FinalizationResult};
pub use gate::{
    GateCoordinator, GateDefinition, GateEligibility, GateExecution, GateRunner, configured_gate,
    configured_gate_names, evaluate_required_gates,
};
pub use planner::{
    DEFAULT_MAX_CONCURRENCY, DispatchPlan, Reservation, ScheduleBlocker, Scheduler,
    SchedulerConfig, queue_blockers_for,
};
pub use policy::{
    MAX_CONCURRENCY_LIMIT, ProjectConcurrency, SchedulerPolicy, SchedulerPolicyService,
};
pub use profiles::ensure_builtin_agent_profiles;
pub use recovery::{RecoveryAction, RecoveryCoordinator, RecoveryRecord};
pub use runtime_control::RuntimeCoordinator;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("scheduler concurrency must be greater than zero")]
    InvalidConcurrency,
    #[error("scheduler concurrency must be between 1 and {maximum}")]
    ConcurrencyOutOfRange { maximum: usize },
    #[error("project concurrency {requested} exceeds global concurrency {global}")]
    InvalidProjectConcurrency { requested: usize, global: usize },
    #[error("invalid project scheduler configuration: {0}")]
    InvalidProjectConfig(String),
    #[error("task is not schedulable: {0}")]
    NotSchedulable(String),
    #[error("task is already active: {0}")]
    AlreadyActive(String),
    #[error("task is not active: {0}")]
    NotActive(String),
    #[error("approval is already resolved: {0}")]
    ApprovalAlreadyResolved(String),
    #[error("approval does not belong to an active run: {0}")]
    ApprovalRunInactive(String),
    #[error("gate definition is invalid: {0}")]
    InvalidGate(String),
    #[error("gate working directory is invalid: {0}")]
    InvalidGateWorkingDirectory(std::path::PathBuf),
    #[error("quality gate process did not expose {0}")]
    MissingGatePipe(&'static str),
    #[error("quality gate output capture failed: {0}")]
    GateCapture(String),
    #[error(transparent)]
    Git(#[from] ba_git::GitError),
    #[error(transparent)]
    Store(#[from] ba_store::StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Runtime(#[from] ba_runtime::RuntimeError),
    #[error(transparent)]
    InvalidEnvironment(#[from] std::env::JoinPathsError),
}

#[cfg(test)]
mod tests;
