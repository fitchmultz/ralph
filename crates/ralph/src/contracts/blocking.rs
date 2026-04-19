//! Purpose: Define canonical operator-facing blocked/waiting/stalled state contracts.
//!
//! Responsibilities:
//! - Provide the stable wire model for why Ralph is not making progress.
//! - Centralize human-readable narration reused by CLI, machine, and app surfaces.
//! - Keep coarse operator state distinct from per-task runnability details.
//!
//! Scope:
//! - Stable serde/schemars contracts and small constructor helpers.
//!
//! Usage:
//! - Construct `BlockingState` values when queue analysis, lock contention, CI, runner/session
//!   recovery, or operator-guided continuation explains the current lack of progress.
//!
//! Invariants/Assumptions:
//! - `BlockingReason` is coarse system-level classification, not a per-task blocker dump.
//! - `message` is the short operator-facing summary; `detail` carries supporting context.
//! - `observed_at` is optional RFC3339 UTC (`Z`) marking when this blocking snapshot was produced;
//!   queue runnability uses the same instant as `QueueRunnabilityReport.now`.
//! - Fields remain machine-safe and versioned through the surrounding contract documents.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlockingStatus {
    Waiting,
    Blocked,
    Stalled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockingReason {
    Idle {
        include_draft: bool,
    },
    DependencyBlocked {
        blocked_tasks: usize,
    },
    ScheduleBlocked {
        blocked_tasks: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        next_runnable_at: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seconds_until_next_runnable: Option<i64>,
    },
    LockBlocked {
        #[serde(skip_serializing_if = "Option::is_none")]
        lock_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        owner: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        owner_pid: Option<u32>,
    },
    CiBlocked {
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },
    RunnerRecovery {
        scope: String,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    OperatorRecovery {
        scope: String,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggested_command: Option<String>,
    },
    MixedQueue {
        dependency_blocked: usize,
        schedule_blocked: usize,
        status_filtered: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BlockingState {
    pub status: BlockingStatus,
    pub reason: BlockingReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub message: String,
    pub detail: String,
    /// When this blocking condition was observed (RFC3339 UTC), for staleness and automation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
}

impl BlockingState {
    pub fn new(
        status: BlockingStatus,
        reason: BlockingReason,
        task_id: Option<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status,
            reason,
            task_id,
            message: message.into(),
            detail: detail.into(),
            observed_at: None,
        }
    }

    /// Attach the instant this blocking state was observed (RFC3339 UTC).
    pub fn with_observed_at(mut self, observed_at: impl Into<String>) -> Self {
        self.observed_at = Some(observed_at.into());
        self
    }

    pub fn idle(include_draft: bool) -> Self {
        let message = if include_draft {
            "Ralph is idle: no todo or draft tasks are available."
        } else {
            "Ralph is idle: no todo tasks are available."
        };
        let detail = if include_draft {
            "The queue currently has no runnable todo or draft candidates; Ralph is waiting for new work."
        } else {
            "The queue currently has no runnable todo candidates; Ralph is waiting for new work."
        };
        Self::new(
            BlockingStatus::Waiting,
            BlockingReason::Idle { include_draft },
            None,
            message,
            detail,
        )
    }

    pub fn dependency_blocked(blocked_tasks: usize) -> Self {
        Self::new(
            BlockingStatus::Blocked,
            BlockingReason::DependencyBlocked { blocked_tasks },
            None,
            "Ralph is blocked by unfinished dependencies.",
            format!("{blocked_tasks} candidate task(s) are waiting on dependency completion."),
        )
    }

    pub fn schedule_blocked(
        blocked_tasks: usize,
        next_runnable_at: Option<String>,
        seconds_until_next_runnable: Option<i64>,
    ) -> Self {
        let detail = match (&next_runnable_at, seconds_until_next_runnable) {
            (Some(next_at), Some(seconds)) => format!(
                "{blocked_tasks} candidate task(s) are scheduled for the future. The next one becomes runnable at {next_at} ({seconds}s remaining)."
            ),
            (Some(next_at), None) => format!(
                "{blocked_tasks} candidate task(s) are scheduled for the future. The next known scheduled time is {next_at}."
            ),
            _ => {
                format!("{blocked_tasks} candidate task(s) are scheduled for the future.")
            }
        };
        Self::new(
            BlockingStatus::Waiting,
            BlockingReason::ScheduleBlocked {
                blocked_tasks,
                next_runnable_at,
                seconds_until_next_runnable,
            },
            None,
            "Ralph is waiting for scheduled work to become runnable.",
            detail,
        )
    }

    pub fn mixed_queue(
        dependency_blocked: usize,
        schedule_blocked: usize,
        status_filtered: usize,
    ) -> Self {
        Self::new(
            BlockingStatus::Blocked,
            BlockingReason::MixedQueue {
                dependency_blocked,
                schedule_blocked,
                status_filtered,
            },
            None,
            "Ralph is blocked by a mix of dependency and schedule gates.",
            format!(
                "candidate blockers: dependencies={dependency_blocked}, schedule={schedule_blocked}, status_or_flags={status_filtered}."
            ),
        )
    }

    pub fn lock_blocked(
        lock_path: Option<String>,
        owner: Option<String>,
        owner_pid: Option<u32>,
    ) -> Self {
        let detail = match (&owner, owner_pid, &lock_path) {
            (Some(owner), Some(owner_pid), Some(lock_path)) => format!(
                "Another Ralph process ({owner}, pid {owner_pid}) owns the queue lock at {lock_path}."
            ),
            (Some(owner), Some(owner_pid), None) => {
                format!("Another Ralph process ({owner}, pid {owner_pid}) owns the queue lock.")
            }
            (_, _, Some(lock_path)) => {
                format!("Another Ralph process owns the queue lock at {lock_path}.")
            }
            _ => "Another Ralph process currently owns the queue lock.".to_string(),
        };
        Self::new(
            BlockingStatus::Stalled,
            BlockingReason::LockBlocked {
                lock_path,
                owner,
                owner_pid,
            },
            None,
            "Ralph is stalled on queue lock contention.",
            detail,
        )
    }

    pub fn ci_blocked(exit_code: Option<i32>, pattern: Option<String>) -> Self {
        let detail = match (&pattern, exit_code) {
            (Some(pattern), Some(exit_code)) => {
                format!("CI gate failed with exit code {exit_code}. Detected pattern: {pattern}.")
            }
            (Some(pattern), None) => format!("CI gate failed. Detected pattern: {pattern}."),
            (None, Some(exit_code)) => {
                format!("CI gate failed with exit code {exit_code}.")
            }
            (None, None) => "CI gate failed without a classified pattern.".to_string(),
        };
        Self::new(
            BlockingStatus::Stalled,
            BlockingReason::CiBlocked { exit_code, pattern },
            None,
            "Ralph is stalled on CI gate failure.",
            detail,
        )
    }

    pub fn runner_recovery(
        scope: impl Into<String>,
        reason: impl Into<String>,
        task_id: Option<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(
            BlockingStatus::Stalled,
            BlockingReason::RunnerRecovery {
                scope: scope.into(),
                reason: reason.into(),
                task_id: task_id.clone(),
            },
            task_id,
            message,
            detail,
        )
    }

    pub fn operator_recovery(
        status: BlockingStatus,
        scope: impl Into<String>,
        reason: impl Into<String>,
        task_id: Option<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
        suggested_command: Option<String>,
    ) -> Self {
        Self::new(
            status,
            BlockingReason::OperatorRecovery {
                scope: scope.into(),
                reason: reason.into(),
                suggested_command,
            },
            task_id,
            message,
            detail,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_at_serializes_and_round_trips() {
        let state = BlockingState::idle(false).with_observed_at("2026-04-19T15:30:00.123456789Z");
        let json = serde_json::to_value(&state).expect("serialize");
        assert_eq!(json["observed_at"], "2026-04-19T15:30:00.123456789Z");
        let back: BlockingState = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, state);
    }

    #[test]
    fn observed_at_defaults_to_none_when_absent_in_json() {
        let json = serde_json::json!({
            "status": "waiting",
            "reason": { "kind": "idle", "include_draft": false },
            "message": "m",
            "detail": "d"
        });
        let state: BlockingState = serde_json::from_value(json).expect("deserialize");
        assert!(state.observed_at.is_none());
    }
}
