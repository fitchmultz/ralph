//! Versioned machine-contract documents for app/CLI integration.
//!
//! Purpose:
//! - Versioned machine-contract documents for app/CLI integration.
//!
//! Responsibilities:
//! - Define the stable JSON documents consumed by the macOS app via `ralph machine ...`.
//! - Centralize machine-only request/response and event envelope types.
//! - Provide schema-friendly wrappers around queue/config/task/run data.
//!
//! Not handled here:
//! - Command execution or clap wiring.
//! - Human CLI rendering.
//! - Queue/task/run business logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Every machine document includes an explicit `version`.
//! - Breaking wire changes require version bumps.
//! - Run events are emitted as NDJSON envelopes ordered by occurrence.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::{
    BlockingState, CliSpec, Config, GitPublishMode, GitRevertMode, QueueFile, RunnerApprovalMode,
    Task,
};

pub const MACHINE_SYSTEM_INFO_VERSION: u32 = 1;
pub const MACHINE_QUEUE_READ_VERSION: u32 = 1;
pub const MACHINE_QUEUE_VALIDATE_VERSION: u32 = 1;
pub const MACHINE_QUEUE_REPAIR_VERSION: u32 = 1;
pub const MACHINE_QUEUE_UNDO_VERSION: u32 = 1;
pub const MACHINE_CONFIG_RESOLVE_VERSION: u32 = 3;
pub const MACHINE_WORKSPACE_OVERVIEW_VERSION: u32 = 1;
pub const MACHINE_TASK_CREATE_VERSION: u32 = 1;
pub const MACHINE_TASK_BUILD_VERSION: u32 = 1;
pub const MACHINE_TASK_MUTATION_VERSION: u32 = 2;
pub const MACHINE_GRAPH_READ_VERSION: u32 = 1;
pub const MACHINE_DASHBOARD_READ_VERSION: u32 = 1;
pub const MACHINE_DECOMPOSE_VERSION: u32 = 2;
pub const MACHINE_RUN_EVENT_VERSION: u32 = 3;
pub const MACHINE_RUN_SUMMARY_VERSION: u32 = 2;
pub const MACHINE_DOCTOR_REPORT_VERSION: u32 = 2;
pub const MACHINE_PARALLEL_STATUS_VERSION: u32 = 3;
pub const MACHINE_CLI_SPEC_VERSION: u32 = 2;
pub const MACHINE_ERROR_VERSION: u32 = 1;
pub const MACHINE_QUEUE_UNLOCK_INSPECT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MachineErrorCode {
    CliUnavailable,
    PermissionDenied,
    ConfigIncompatible,
    ParseError,
    NetworkError,
    QueueCorrupted,
    ResourceBusy,
    VersionMismatch,
    TaskMutationConflict,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineErrorDocument {
    pub version: u32,
    pub code: MachineErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineSystemInfoDocument {
    pub version: u32,
    pub cli_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineQueuePaths {
    pub repo_root: String,
    pub queue_path: String,
    pub done_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_config_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineQueueReadDocument {
    pub version: u32,
    pub paths: MachineQueuePaths,
    pub active: QueueFile,
    pub done: QueueFile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_runnable_task_id: Option<String>,
    #[schemars(schema_with = "json_value_schema")]
    pub runnability: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineContinuationAction {
    pub title: String,
    pub command: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineContinuationSummary {
    pub headline: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    #[serde(default)]
    pub next_steps: Vec<MachineContinuationAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineValidationWarning {
    pub task_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineQueueValidateDocument {
    pub version: u32,
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    #[serde(default)]
    pub warnings: Vec<MachineValidationWarning>,
    pub continuation: MachineContinuationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineQueueRepairDocument {
    pub version: u32,
    pub dry_run: bool,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    #[schemars(schema_with = "json_value_schema")]
    pub report: JsonValue,
    pub continuation: MachineContinuationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineQueueUndoDocument {
    pub version: u32,
    pub dry_run: bool,
    pub restored: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    #[schemars(schema_with = "option_json_value_schema")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    pub continuation: MachineContinuationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MachineQueueUnlockCondition {
    Clear,
    Live,
    Stale,
    OwnerMissing,
    OwnerUnreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineQueueUnlockInspectDocument {
    pub version: u32,
    pub condition: MachineQueueUnlockCondition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    pub unlock_allowed: bool,
    pub continuation: MachineContinuationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineResumeDecision {
    pub status: String,
    pub scope: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub message: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineConfigResolveDocument {
    pub version: u32,
    pub paths: MachineQueuePaths,
    pub safety: MachineConfigSafetySummary,
    pub config: Config,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_preview: Option<MachineResumeDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineWorkspaceOverviewDocument {
    pub version: u32,
    pub queue: MachineQueueReadDocument,
    pub config: MachineConfigResolveDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineConfigSafetySummary {
    pub repo_trusted: bool,
    pub dirty_repo: bool,
    pub git_publish_mode: GitPublishMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<RunnerApprovalMode>,
    pub ci_gate_enabled: bool,
    pub git_revert_mode: GitRevertMode,
    pub parallel_configured: bool,
    pub execution_interactivity: String,
    pub interactive_approval_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineCliSpecDocument {
    pub version: u32,
    pub spec: CliSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineTaskCreateRequest {
    pub version: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub priority: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineTaskCreateDocument {
    pub version: u32,
    pub task: Task,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineTaskBuildRequest {
    pub version: u32,
    pub request: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default)]
    pub strict_templates: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineTaskBuildDocument {
    pub version: u32,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    pub result: MachineTaskBuildResult,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub continuation: MachineContinuationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineTaskBuildResult {
    pub created_count: usize,
    #[serde(default)]
    pub task_ids: Vec<String>,
    #[serde(default)]
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineTaskMutationDocument {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    #[schemars(schema_with = "json_value_schema")]
    pub report: JsonValue,
    pub continuation: MachineContinuationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineGraphReadDocument {
    pub version: u32,
    #[schemars(schema_with = "json_value_schema")]
    pub graph: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineDashboardReadDocument {
    pub version: u32,
    #[schemars(schema_with = "json_value_schema")]
    pub dashboard: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineDecomposeDocument {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    #[schemars(schema_with = "json_value_schema")]
    pub result: JsonValue,
    pub continuation: MachineContinuationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineDoctorReportDocument {
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    #[schemars(schema_with = "json_value_schema")]
    pub report: JsonValue,
}

/// Worker counts by lifecycle for `machine run parallel-status` (document v3+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineParallelLifecycleCounts {
    pub running: u32,
    pub integrating: u32,
    pub completed: u32,
    pub failed: u32,
    pub blocked: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineParallelStatusDocument {
    pub version: u32,
    pub lifecycle_counts: MachineParallelLifecycleCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
    pub continuation: MachineContinuationSummary,
    #[schemars(schema_with = "json_value_schema")]
    pub status: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MachineRunEventKind {
    RunStarted,
    QueueSnapshot,
    ConfigResolved,
    ResumeDecision,
    TaskSelected,
    PhaseEntered,
    PhaseCompleted,
    RunnerOutput,
    BlockedStateChanged,
    BlockedStateCleared,
    Warning,
    RunFinished,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineRunEventEnvelope {
    pub version: u32,
    pub kind: MachineRunEventKind,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[schemars(schema_with = "option_json_value_schema")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineRunSummaryDocument {
    pub version: u32,
    pub task_id: Option<String>,
    pub exit_code: i32,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<BlockingState>,
}

fn json_value_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <JsonValue as JsonSchema>::json_schema(generator)
}

fn option_json_value_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <Option<JsonValue> as JsonSchema>::json_schema(generator)
}
