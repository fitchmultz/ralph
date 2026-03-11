//! Versioned machine-contract documents for app/CLI integration.
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
//! Invariants/assumptions:
//! - Every machine document includes an explicit `version`.
//! - Breaking wire changes require version bumps.
//! - Run events are emitted as NDJSON envelopes ordered by occurrence.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::{CliSpec, Config, QueueFile, Task};

pub const MACHINE_SYSTEM_INFO_VERSION: u32 = 1;
pub const MACHINE_QUEUE_READ_VERSION: u32 = 1;
pub const MACHINE_CONFIG_RESOLVE_VERSION: u32 = 1;
pub const MACHINE_TASK_CREATE_VERSION: u32 = 1;
pub const MACHINE_TASK_MUTATION_VERSION: u32 = 1;
pub const MACHINE_GRAPH_READ_VERSION: u32 = 1;
pub const MACHINE_DASHBOARD_READ_VERSION: u32 = 1;
pub const MACHINE_DECOMPOSE_VERSION: u32 = 1;
pub const MACHINE_RUN_EVENT_VERSION: u32 = 1;
pub const MACHINE_RUN_SUMMARY_VERSION: u32 = 1;
pub const MACHINE_DOCTOR_REPORT_VERSION: u32 = 1;
pub const MACHINE_PARALLEL_STATUS_VERSION: u32 = 1;
pub const MACHINE_CLI_SPEC_VERSION: u32 = 1;

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
pub struct MachineConfigResolveDocument {
    pub version: u32,
    pub paths: MachineQueuePaths,
    pub config: Config,
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
pub struct MachineTaskMutationDocument {
    pub version: u32,
    #[schemars(schema_with = "json_value_schema")]
    pub report: JsonValue,
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
    #[schemars(schema_with = "json_value_schema")]
    pub result: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineDoctorReportDocument {
    pub version: u32,
    #[schemars(schema_with = "json_value_schema")]
    pub report: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MachineParallelStatusDocument {
    pub version: u32,
    #[schemars(schema_with = "json_value_schema")]
    pub status: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MachineRunEventKind {
    RunStarted,
    QueueSnapshot,
    ConfigResolved,
    TaskSelected,
    PhaseEntered,
    PhaseCompleted,
    RunnerOutput,
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
}

fn json_value_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <JsonValue as JsonSchema>::json_schema(generator)
}

fn option_json_value_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <Option<JsonValue> as JsonSchema>::json_schema(generator)
}
