//! Domain model and invariants for BA Agent.

mod entity;
mod id;
mod receipt;
mod status;

pub use entity::*;
pub use id::{EntityId, IdError};
pub use receipt::{ReceiptChainError, ReceiptDraft, canonical_json, compute_receipt_hash};
pub use status::{
    ApprovalStatus, GateStatus, RunStatus, StateTransitionError, TaskStatus, WorktreeStatus,
};

/// UTC timestamp used by every persisted entity.
pub type Timestamp = chrono::DateTime<chrono::Utc>;

/// Return the current UTC time.
#[must_use]
pub fn now() -> Timestamp {
    chrono::Utc::now()
}
