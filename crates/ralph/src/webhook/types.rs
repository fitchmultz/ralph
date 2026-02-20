//! Webhook type definitions.
//!
//! Responsibilities:
//! - Define webhook event types, payloads, and context structures.
//! - Provide resolved configuration types.
//!
//! Not handled here:
//! - Delivery logic (see `super::worker`).
//! - Notification convenience functions (see `super::notifications`).

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::contracts::WebhookConfig;

/// Types of webhook events that can be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookEventType {
    /// Task was created/added to queue.
    TaskCreated,
    /// Task status changed to Doing (execution started).
    TaskStarted,
    /// Task completed successfully (status Done).
    TaskCompleted,
    /// Task failed or was rejected.
    TaskFailed,
    /// Generic status change (used when specific type not applicable).
    TaskStatusChanged,
    /// Run loop started.
    LoopStarted,
    /// Run loop stopped (success, failure, or signal).
    LoopStopped,
    /// Phase started for a task.
    PhaseStarted,
    /// Phase completed for a task.
    PhaseCompleted,
    /// Queue became unblocked (runnable tasks available after being blocked).
    QueueUnblocked,
}

impl WebhookEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WebhookEventType::TaskCreated => "task_created",
            WebhookEventType::TaskStarted => "task_started",
            WebhookEventType::TaskCompleted => "task_completed",
            WebhookEventType::TaskFailed => "task_failed",
            WebhookEventType::TaskStatusChanged => "task_status_changed",
            WebhookEventType::LoopStarted => "loop_started",
            WebhookEventType::LoopStopped => "loop_stopped",
            WebhookEventType::PhaseStarted => "phase_started",
            WebhookEventType::PhaseCompleted => "phase_completed",
            WebhookEventType::QueueUnblocked => "queue_unblocked",
        }
    }
}

impl std::str::FromStr for WebhookEventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "task_created" => Self::TaskCreated,
            "task_started" => Self::TaskStarted,
            "task_completed" => Self::TaskCompleted,
            "task_failed" => Self::TaskFailed,
            "task_status_changed" => Self::TaskStatusChanged,
            "loop_started" => Self::LoopStarted,
            "loop_stopped" => Self::LoopStopped,
            "phase_started" => Self::PhaseStarted,
            "phase_completed" => Self::PhaseCompleted,
            "queue_unblocked" => Self::QueueUnblocked,
            other => anyhow::bail!(
                "Unknown event type: {}. Supported: task_created, task_started, task_completed, task_failed, task_status_changed, loop_started, loop_stopped, phase_started, phase_completed, queue_unblocked",
                other
            ),
        })
    }
}

/// Optional context metadata for webhook payloads.
/// These fields are only serialized when set (Some).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebhookContext {
    /// Runner used for this phase/execution (e.g., "claude", "codex").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    /// Model used for this phase/execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Current phase number (1, 2, or 3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<u8>,
    /// Total number of phases configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_count: Option<u8>,
    /// Duration in milliseconds (for completed operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Repository root path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    /// Current git branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Current git commit hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// CI gate outcome: "skipped", "passed", or "failed".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_gate: Option<String>,
}

/// Webhook event payload structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    /// Event type identifier.
    pub event: String,
    /// Timestamp of the event (RFC3339).
    pub timestamp: String,
    /// Task ID (e.g., "RQ-0001").
    /// Optional: may be None for loop-level events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Task title.
    /// Optional: may be None for loop-level events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    /// Previous status (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_status: Option<String>,
    /// Current/new status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_status: Option<String>,
    /// Additional context or notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Optional context metadata (runner, model, phase, git info, etc.)
    #[serde(flatten)]
    pub context: WebhookContext,
}

/// Resolved webhook configuration with defaults applied.
#[derive(Debug, Clone)]
pub struct ResolvedWebhookConfig {
    pub enabled: bool,
    pub url: Option<String>,
    pub secret: Option<String>,
    pub timeout: Duration,
    pub retry_count: u32,
    pub retry_backoff: Duration,
}

impl ResolvedWebhookConfig {
    /// Resolve a WebhookConfig to concrete values with defaults.
    pub fn from_config(config: &WebhookConfig) -> Self {
        Self {
            enabled: config.enabled.unwrap_or(false),
            url: config.url.clone(),
            secret: config.secret.clone(),
            timeout: Duration::from_secs(config.timeout_secs.unwrap_or(30) as u64),
            retry_count: config.retry_count.unwrap_or(3),
            retry_backoff: Duration::from_millis(config.retry_backoff_ms.unwrap_or(1000) as u64),
        }
    }
}

/// Internal message for the webhook worker.
#[derive(Debug, Clone)]
pub(crate) struct WebhookMessage {
    pub(crate) payload: WebhookPayload,
    pub(crate) config: ResolvedWebhookConfig,
}

/// Resolve webhook config to concrete values.
pub(crate) fn resolve_webhook_config(config: &WebhookConfig) -> ResolvedWebhookConfig {
    ResolvedWebhookConfig::from_config(config)
}
