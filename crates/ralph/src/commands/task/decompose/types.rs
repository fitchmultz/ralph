//! Shared task decomposition data models.
//!
//! Purpose:
//! - Shared task decomposition data models.
//!
//! Responsibilities:
//! - Define the public preview/write types exposed to CLI and machine consumers.
//! - Hold planner-response parsing structs shared by normalization helpers.
//! - Keep decomposition-only internal state localized away from the facade module.
//!
//! Not handled here:
//! - Runner invocation, prompt rendering, or queue mutation logic.
//! - Tree normalization algorithms or task materialization helpers.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Serialized public types remain stable for current CLI and machine output contracts.
//! - Internal planner structs mirror the planner JSON schema with unknown fields rejected.

use crate::contracts::{Model, ReasoningEffort, Runner, Task, TaskStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecompositionChildPolicy {
    Fail,
    Append,
    Replace,
}

#[derive(Debug, Clone)]
pub struct TaskDecomposeOptions {
    pub source_input: String,
    pub attach_to_task_id: Option<String>,
    pub max_depth: u8,
    pub max_children: usize,
    pub max_nodes: usize,
    pub status: TaskStatus,
    pub child_policy: DecompositionChildPolicy,
    pub with_dependencies: bool,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch,
    pub repoprompt_tool_injection: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecompositionSource {
    Freeform { request: String },
    ExistingTask { task: Box<Task> },
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompositionAttachTarget {
    pub task: Box<Task>,
    pub has_existing_children: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompositionPreview {
    pub source: DecompositionSource,
    pub attach_target: Option<DecompositionAttachTarget>,
    pub plan: DecompositionPlan,
    pub write_blockers: Vec<String>,
    pub child_status: TaskStatus,
    pub child_policy: DecompositionChildPolicy,
    pub with_dependencies: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecompositionPlan {
    pub root: PlannedNode,
    pub warnings: Vec<String>,
    pub total_nodes: usize,
    pub leaf_nodes: usize,
    pub dependency_edges: Vec<DependencyEdgePreview>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyEdgePreview {
    pub task_title: String,
    pub depends_on_title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDecomposeWriteResult {
    pub root_task_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub created_ids: Vec<String>,
    pub replaced_ids: Vec<String>,
    pub parent_annotated: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDecompositionResponse {
    #[serde(default)]
    pub(super) warnings: Vec<String>,
    pub(super) tree: RawPlannedNode,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPlannedNode {
    #[serde(default)]
    pub(super) key: Option<String>,
    pub(super) title: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) plan: Vec<String>,
    #[serde(default)]
    pub(super) tags: Vec<String>,
    #[serde(default)]
    pub(super) scope: Vec<String>,
    #[serde(default)]
    pub(super) depends_on: Vec<String>,
    #[serde(default)]
    pub(super) children: Vec<RawPlannedNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedNode {
    pub planner_key: String,
    pub title: String,
    pub description: Option<String>,
    pub plan: Vec<String>,
    pub tags: Vec<String>,
    pub scope: Vec<String>,
    pub depends_on_keys: Vec<String>,
    pub children: Vec<PlannedNode>,
    #[serde(skip_serializing)]
    pub(super) dependency_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceKind {
    Freeform,
    ExistingTask,
}

pub(super) struct PlannerState {
    pub(super) remaining_nodes: usize,
    pub(super) warnings: Vec<String>,
    pub(super) with_dependencies: bool,
}
