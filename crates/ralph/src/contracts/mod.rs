//! Contracts module for Ralph configuration and queue/task JSON structures.
//!
//! Responsibilities:
//! - Own the canonical data models for config, queue, and task contracts.
//! - Re-export the public contract types for crate-wide access.
//!
//! Not handled here:
//! - Queue persistence and IO (see `crate::queue`).
//! - CLI argument parsing or command behavior (see `crate::cli`).
//!
//! Invariants/assumptions:
//! - Public contract types remain stable and are re-exported from this module.
//! - Serde/schemars attributes define the wire contract and must not drift.

#![allow(clippy::struct_excessive_bools)]

mod config;
mod queue;
mod session;
mod task;

pub use config::{
    AgentConfig, AutoArchiveBehavior, ClaudePermissionMode, Config, GitRevertMode, Model,
    ModelEffort, NotificationConfig, PhaseOverrideConfig, PhaseOverrides, ProjectType, QueueConfig,
    ReasoningEffort, Runner, RunnerApprovalMode, RunnerCliConfigRoot, RunnerCliOptionsPatch,
    RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity, TuiConfig,
    UnsupportedOptionPolicy, WebhookConfig,
};
pub use queue::QueueFile;
pub use session::{PhaseSettingsSnapshot, SessionState};

// Re-export SESSION_STATE_VERSION from constants for backward compatibility
pub use crate::constants::versions::SESSION_STATE_VERSION;
pub use task::{Task, TaskAgent, TaskPriority, TaskStatus};
