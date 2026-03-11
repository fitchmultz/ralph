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

mod cli_spec;
mod config;
mod machine;
mod model;
mod queue;
mod runner;
mod session;
mod task;

// Re-exports from config module (core config types)
// All config types are now re-exported from config::mod.rs for backward compatibility
pub use config::{
    AgentConfig, CiGateConfig, Config, GitRevertMode, LoopConfig, NotificationConfig,
    ParallelConfig, PhaseOverrideConfig, PhaseOverrides, PluginConfig, PluginsConfig, ProjectType,
    QueueAgingThresholds, QueueConfig, RunnerRetryConfig, ScanPromptVersion, WebhookConfig,
    WebhookEventSubscription, WebhookQueuePolicy,
};

// Re-exports from machine module (versioned app/CLI machine surfaces)
pub use machine::{
    MACHINE_CLI_SPEC_VERSION, MACHINE_CONFIG_RESOLVE_VERSION, MACHINE_DASHBOARD_READ_VERSION,
    MACHINE_DECOMPOSE_VERSION, MACHINE_DOCTOR_REPORT_VERSION, MACHINE_GRAPH_READ_VERSION,
    MACHINE_PARALLEL_STATUS_VERSION, MACHINE_QUEUE_READ_VERSION, MACHINE_RUN_EVENT_VERSION,
    MACHINE_RUN_SUMMARY_VERSION, MACHINE_SYSTEM_INFO_VERSION, MACHINE_TASK_CREATE_VERSION,
    MACHINE_TASK_MUTATION_VERSION, MachineCliSpecDocument, MachineConfigResolveDocument,
    MachineDashboardReadDocument, MachineDecomposeDocument, MachineDoctorReportDocument,
    MachineGraphReadDocument, MachineParallelStatusDocument, MachineQueuePaths,
    MachineQueueReadDocument, MachineRunEventEnvelope, MachineRunEventKind,
    MachineRunSummaryDocument, MachineSystemInfoDocument, MachineTaskCreateDocument,
    MachineTaskCreateRequest, MachineTaskMutationDocument,
};

// Re-exports from cli_spec module (versioned; suitable for tooling consumption)
pub use cli_spec::{ArgSpec, CLI_SPEC_VERSION, CliSpec, CommandSpec};

// Re-exports from model module (model types)
pub use model::{Model, ModelEffort, ReasoningEffort};

// Re-exports from queue module
pub use queue::QueueFile;

// Re-exports from runner module (runner types)
pub use runner::{
    ClaudePermissionMode, Runner, RunnerApprovalMode, RunnerCliConfigRoot, RunnerCliOptionsPatch,
    RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
    UnsupportedOptionPolicy,
};

// Re-exports from session module
pub use session::{PhaseSettingsSnapshot, SessionState};

// Re-export SESSION_STATE_VERSION from constants for backward compatibility
pub use crate::constants::versions::SESSION_STATE_VERSION;

// Re-exports from task module
pub use task::{Task, TaskAgent, TaskPriority, TaskStatus};
