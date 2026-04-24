//! Integration configuration and handoff data structures.
//!
//! Purpose:
//! - Integration configuration and handoff data structures.
//!
//! Responsibilities:
//! - Define reusable integration-loop configuration and outcome types.
//! - Define blocked-push marker and remediation handoff payloads.
//!
//! Non-scope:
//! - Persistence to disk.
//! - Compliance validation or retry orchestration.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::config::Resolved;
use crate::runutil::FixedBackoffSchedule;
use crate::timeutil;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the integration loop.
#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    /// Maximum number of integration attempts.
    pub max_attempts: u32,
    /// Explicit retry schedule shared with runtime retry helpers.
    pub(crate) backoff_schedule: FixedBackoffSchedule,
    /// Target branch to push to.
    pub target_branch: String,
    /// Whether CI gate is enabled.
    pub ci_enabled: bool,
    /// Rendered CI gate label for prompts and handoff context.
    pub ci_label: String,
}

impl IntegrationConfig {
    pub fn from_resolved(resolved: &Resolved, target_branch: &str) -> Self {
        let parallel = &resolved.config.parallel;
        let target_branch = target_branch.trim();
        Self {
            max_attempts: parallel.max_push_attempts.unwrap_or(50) as u32,
            backoff_schedule: FixedBackoffSchedule::from_millis(
                &parallel
                    .push_backoff_ms
                    .clone()
                    .unwrap_or_else(super::super::default_push_backoff_ms),
            ),
            target_branch: if target_branch.is_empty() {
                "main".to_string()
            } else {
                target_branch.to_string()
            },
            ci_enabled: resolved
                .config
                .agent
                .ci_gate
                .as_ref()
                .is_some_and(|ci_gate| ci_gate.is_enabled()),
            ci_label: crate::commands::run::supervision::ci_gate_command_label(resolved),
        }
    }

    /// Get backoff for a specific attempt index (0-indexed).
    pub fn backoff_for_attempt(&self, attempt: usize) -> Duration {
        self.backoff_schedule.delay_for_retry(attempt)
    }
}

/// Outcome of the integration loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationOutcome {
    /// Push succeeded and compliance gates passed.
    Success,
    /// Integration could not complete within bounded retries.
    BlockedPush { reason: String },
    /// Terminal integration failure (for example no resumable session).
    Failed { reason: String },
}

/// Persisted marker written by workers when integration ends in `blocked_push`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BlockedPushMarker {
    pub task_id: String,
    pub reason: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub generated_at: String,
}

/// Structured handoff packet for blocked remediation attempts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationHandoff {
    pub task_id: String,
    pub task_title: String,
    pub target_branch: String,
    pub attempt: u32,
    pub max_attempts: u32,
    pub conflict_files: Vec<String>,
    pub git_status: String,
    pub phase_summary: String,
    pub task_intent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_context: Option<CiContext>,
    pub generated_at: String,
    pub queue_done_rules: QueueDoneRules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiContext {
    pub command: String,
    pub last_output: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueDoneRules {
    pub rules: Vec<String>,
}

impl Default for QueueDoneRules {
    fn default() -> Self {
        Self {
            rules: vec![
                "Remove the completed task from the queue file".into(),
                "Ensure the completed task is present in the done archive file".into(),
                "Preserve entries from other workers exactly".into(),
                "Preserve task metadata (timestamps/notes)".into(),
            ],
        }
    }
}

impl RemediationHandoff {
    pub fn new(
        task_id: impl Into<String>,
        task_title: impl Into<String>,
        target_branch: impl Into<String>,
        attempt: u32,
        max_attempts: u32,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            task_title: task_title.into(),
            target_branch: target_branch.into(),
            attempt,
            max_attempts,
            conflict_files: Vec::new(),
            git_status: String::new(),
            phase_summary: String::new(),
            task_intent: String::new(),
            ci_context: None,
            generated_at: timeutil::now_utc_rfc3339_or_fallback(),
            queue_done_rules: QueueDoneRules::default(),
        }
    }

    pub fn with_conflicts(mut self, files: Vec<String>) -> Self {
        self.conflict_files = files;
        self
    }

    pub fn with_git_status(mut self, status: String) -> Self {
        self.git_status = status;
        self
    }

    pub fn with_phase_summary(mut self, summary: String) -> Self {
        self.phase_summary = summary;
        self
    }

    pub fn with_task_intent(mut self, intent: String) -> Self {
        self.task_intent = intent;
        self
    }

    pub fn with_ci_context(mut self, command: String, last_output: String, exit_code: i32) -> Self {
        self.ci_context = Some(CiContext {
            command,
            last_output,
            exit_code,
        });
        self
    }
}
