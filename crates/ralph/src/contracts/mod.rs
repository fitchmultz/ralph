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
mod model;
mod queue;
mod runner;
mod session;
mod task;

// Re-exports from config module (core config types)
pub use config::{
    AgentConfig, AutoArchiveBehavior, Config, ConflictPolicy, GitRevertMode, NotificationConfig,
    ParallelConfig, ParallelMergeMethod, ParallelMergeWhen, PhaseOverrideConfig, PhaseOverrides,
    ProjectType, QueueConfig, ScanPromptVersion, TuiConfig, WebhookConfig,
};

// Re-exports from model module (model types)
pub use model::{Model, ModelEffort, ReasoningEffort};

// Re-exports from queue module
pub use queue::QueueFile;

// Re-exports from runner module (runner types)
pub use runner::{
    ClaudePermissionMode, MergeRunnerConfig, Runner, RunnerApprovalMode, RunnerCliConfigRoot,
    RunnerCliOptionsPatch, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
    UnsupportedOptionPolicy,
};

// Re-exports from session module
pub use session::{PhaseSettingsSnapshot, SessionState};

// Re-export SESSION_STATE_VERSION from constants for backward compatibility
pub use crate::constants::versions::SESSION_STATE_VERSION;

// Re-exports from task module
pub use task::{Task, TaskAgent, TaskPriority, TaskStatus};
