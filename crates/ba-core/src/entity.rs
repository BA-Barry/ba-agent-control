use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ApprovalStatus, EntityId, GateStatus, RunStatus, TaskStatus, Timestamp, WorktreeStatus,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: EntityId,
    pub name: String,
    pub root_path: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_language")]
    pub language: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workstream {
    pub id: EntityId,
    pub project_id: EntityId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_active")]
    pub status: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A user-controlled, bounded implementation/review/repair workflow.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewRepairLoop {
    pub id: EntityId,
    pub project_id: EntityId,
    pub workstream_id: EntityId,
    pub title: String,
    #[serde(default)]
    pub objective: String,
    pub write_scope: Vec<String>,
    pub max_repairs: u8,
    #[serde(default)]
    pub repairs_started: u8,
    pub status: ReviewRepairLoopStatus,
    pub active_review_task_id: EntityId,
    #[serde(default)]
    pub confirmation_task_id: Option<EntityId>,
    #[serde(default)]
    pub decisions: Vec<ReviewRepairDecision>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRepairLoopStatus {
    Running,
    AwaitingDecision,
    Confirming,
    Completed,
    Blocked,
    Exhausted,
}

impl ReviewRepairLoopStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::AwaitingDecision => "awaiting_decision",
            Self::Confirming => "confirming",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Exhausted => "exhausted",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewRepairDecision {
    pub decision: String,
    pub review_task_id: EntityId,
    #[serde(default)]
    pub note: String,
    pub decided_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Milestone {
    pub id: EntityId,
    pub project_id: EntityId,
    pub workstream_id: Option<EntityId>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_planned")]
    pub status: String,
    pub due_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: EntityId,
    pub project_id: EntityId,
    pub workstream_id: Option<EntityId>,
    pub milestone_id: Option<EntityId>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub write: bool,
    #[serde(default)]
    pub path_scope: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<EntityId>,
    /// Durable role assignment selected during planning or template instantiation.
    #[serde(default)]
    pub agent_profile_id: Option<EntityId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// One immutable user decision that changes whether a Task appears in the active register.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskDisposition {
    pub id: EntityId,
    pub task_id: EntityId,
    pub action: TaskDispositionAction,
    pub reason: String,
    #[serde(default)]
    pub replacement_task_id: Option<EntityId>,
    #[serde(default)]
    pub previous_disposition_id: Option<EntityId>,
    pub created_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDispositionAction {
    Archived,
    Superseded,
    Restored,
}

impl TaskDispositionAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Archived => "archived",
            Self::Superseded => "superseded",
            Self::Restored => "restored",
        }
    }

    #[must_use]
    pub const fn hides_task(self) -> bool {
        matches!(self, Self::Archived | Self::Superseded)
    }
}

/// An inactive Task paired with the immutable decision that currently hides it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InactiveTask {
    pub task: Task,
    pub disposition: TaskDisposition,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: EntityId,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default = "default_codex")]
    pub runtime: String,
    #[serde(default)]
    pub config: Value,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub id: EntityId,
    pub task_id: EntityId,
    pub agent_profile_id: Option<EntityId>,
    pub status: RunStatus,
    pub adapter: String,
    #[serde(default)]
    pub runtime_run_id: Option<String>,
    pub runtime_thread_id: Option<String>,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dispatch {
    pub id: EntityId,
    pub task_id: EntityId,
    pub run_id: EntityId,
    pub agent_profile_id: Option<EntityId>,
    #[serde(default = "default_queued")]
    pub status: String,
    /// Stable user-controlled order in the global durable queue.
    #[serde(default)]
    pub queue_position: i64,
    /// Optional prompt supplied by the dispatching control surface.
    #[serde(default)]
    pub prompt: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Approval {
    pub id: EntityId,
    pub run_id: EntityId,
    pub kind: String,
    pub status: ApprovalStatus,
    #[serde(default)]
    pub request: Value,
    pub decision: Option<Value>,
    pub requested_at: Timestamp,
    pub decided_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gate {
    pub id: EntityId,
    pub task_id: EntityId,
    pub run_id: Option<EntityId>,
    pub name: String,
    pub status: GateStatus,
    pub command: Option<String>,
    pub result: Option<Value>,
    pub created_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub id: EntityId,
    pub project_id: EntityId,
    pub task_id: Option<EntityId>,
    pub run_id: Option<EntityId>,
    pub kind: String,
    pub payload: Value,
    pub previous_hash: Option<String>,
    pub hash: String,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub id: EntityId,
    pub project_id: EntityId,
    pub task_id: Option<EntityId>,
    pub run_id: Option<EntityId>,
    pub kind: String,
    pub path: String,
    pub digest: Option<String>,
    /// Structured, kind-specific metadata. Artifact bytes remain in the referenced file.
    #[serde(default)]
    pub metadata: Value,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Worktree {
    pub id: EntityId,
    pub project_id: EntityId,
    pub task_id: EntityId,
    pub run_id: Option<EntityId>,
    pub path: String,
    #[serde(default)]
    pub base_revision: Option<String>,
    #[serde(default)]
    pub write_scope: Vec<String>,
    pub branch: Option<String>,
    pub status: WorktreeStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunEvent {
    pub run_id: EntityId,
    pub sequence: u64,
    pub kind: String,
    pub payload: Value,
    pub created_at: Timestamp,
}

fn default_language() -> String {
    "zh-CN".to_owned()
}

fn default_active() -> String {
    "active".to_owned()
}

fn default_planned() -> String {
    "planned".to_owned()
}

fn default_codex() -> String {
    "codex".to_owned()
}

fn default_queued() -> String {
    "queued".to_owned()
}
