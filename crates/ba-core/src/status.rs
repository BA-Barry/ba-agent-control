use serde::{Deserialize, Serialize};
use thiserror::Error;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = StateTransitionError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(StateTransitionError::UnknownStatus(value.to_owned())),
                }
            }
        }
    };
}

string_enum!(TaskStatus {
    Draft => "draft",
    Ready => "ready",
    Queued => "queued",
    Running => "running",
    Review => "review",
    Completed => "completed",
    Blocked => "blocked",
    Failed => "failed",
    Cancelled => "cancelled",
});

impl TaskStatus {
    /// Whether this state may move directly to `next`.
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        if self as u8 == next as u8 {
            return true;
        }
        match self {
            Self::Draft => matches!(next, Self::Ready | Self::Cancelled),
            Self::Ready => matches!(
                next,
                Self::Draft | Self::Queued | Self::Blocked | Self::Cancelled
            ),
            Self::Queued => matches!(
                next,
                Self::Running | Self::Blocked | Self::Failed | Self::Cancelled
            ),
            Self::Running => matches!(
                next,
                Self::Review | Self::Blocked | Self::Failed | Self::Cancelled
            ),
            Self::Review => matches!(
                next,
                Self::Running | Self::Completed | Self::Blocked | Self::Failed | Self::Cancelled
            ),
            Self::Blocked => matches!(
                next,
                Self::Ready | Self::Queued | Self::Running | Self::Cancelled
            ),
            Self::Failed => matches!(next, Self::Ready | Self::Queued | Self::Cancelled),
            Self::Completed | Self::Cancelled => false,
        }
    }

    /// Validate a task status transition.
    ///
    /// # Errors
    ///
    /// Returns [`StateTransitionError::Invalid`] when `next` is not reachable directly.
    pub fn ensure_transition(self, next: Self) -> Result<(), StateTransitionError> {
        if self.can_transition_to(next) {
            Ok(())
        } else {
            Err(StateTransitionError::Invalid {
                entity: "task",
                from: self.to_string(),
                to: next.to_string(),
            })
        }
    }
}

string_enum!(RunStatus {
    Queued => "queued",
    Starting => "starting",
    Running => "running",
    WaitingApproval => "waiting_approval",
    Succeeded => "succeeded",
    Failed => "failed",
    Cancelled => "cancelled",
    Orphaned => "orphaned",
});

impl RunStatus {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        if self as u8 == next as u8 {
            return true;
        }
        match self {
            Self::Queued => matches!(next, Self::Starting | Self::Cancelled),
            Self::Starting => matches!(
                next,
                Self::Running | Self::Failed | Self::Cancelled | Self::Orphaned
            ),
            Self::Running => matches!(
                next,
                Self::WaitingApproval
                    | Self::Succeeded
                    | Self::Failed
                    | Self::Cancelled
                    | Self::Orphaned
            ),
            Self::WaitingApproval => matches!(
                next,
                Self::Running | Self::Failed | Self::Cancelled | Self::Orphaned
            ),
            Self::Orphaned => matches!(
                next,
                Self::Queued | Self::Running | Self::Failed | Self::Cancelled
            ),
            Self::Succeeded | Self::Failed | Self::Cancelled => false,
        }
    }

    /// Validate a run status transition.
    ///
    /// # Errors
    ///
    /// Returns [`StateTransitionError::Invalid`] when `next` is not reachable directly.
    pub fn ensure_transition(self, next: Self) -> Result<(), StateTransitionError> {
        if self.can_transition_to(next) {
            Ok(())
        } else {
            Err(StateTransitionError::Invalid {
                entity: "run",
                from: self.to_string(),
                to: next.to_string(),
            })
        }
    }
}

string_enum!(ApprovalStatus {
    Pending => "pending",
    Approved => "approved",
    Rejected => "rejected",
    Cancelled => "cancelled",
});

string_enum!(GateStatus {
    Pending => "pending",
    Running => "running",
    Passed => "passed",
    Failed => "failed",
    Skipped => "skipped",
});

string_enum!(WorktreeStatus {
    Detached => "detached",
    Active => "active",
    Committed => "committed",
    Retained => "retained",
    Removed => "removed",
});

#[derive(Debug, Error)]
pub enum StateTransitionError {
    #[error("unknown status: {0}")]
    UnknownStatus(String),
    #[error("invalid {entity} state transition: {from} -> {to}")]
    Invalid {
        entity: &'static str,
        from: String,
        to: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{RunStatus, TaskStatus};

    #[test]
    fn task_happy_path_is_valid() -> Result<(), Box<dyn std::error::Error>> {
        let path = [
            TaskStatus::Draft,
            TaskStatus::Ready,
            TaskStatus::Queued,
            TaskStatus::Running,
            TaskStatus::Review,
            TaskStatus::Completed,
        ];
        for states in path.windows(2) {
            states[0].ensure_transition(states[1])?;
        }
        Ok(())
    }

    #[test]
    fn terminal_task_cannot_reopen() {
        assert!(
            TaskStatus::Completed
                .ensure_transition(TaskStatus::Running)
                .is_err()
        );
    }

    #[test]
    fn approval_pause_can_resume() -> Result<(), Box<dyn std::error::Error>> {
        RunStatus::Running.ensure_transition(RunStatus::WaitingApproval)?;
        RunStatus::WaitingApproval.ensure_transition(RunStatus::Running)?;
        Ok(())
    }
}
