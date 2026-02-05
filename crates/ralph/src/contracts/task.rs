//! Task contracts for Ralph queue entries.
//!
//! Responsibilities:
//! - Define task payloads, enums, and schema helpers.
//! - Provide ordering/cycling helpers for task priority.
//!
//! Not handled here:
//! - Queue ordering or persistence logic (see `crate::queue`).
//! - Config contract definitions (see `super::config`).
//!
//! Invariants/assumptions:
//! - Serde/schemars attributes define the task wire contract.
//! - Task priority ordering is critical > high > medium > low.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use super::RunnerCliOptionsPatch;
use super::{Model, ModelEffort, ReasoningEffort, Runner};

/* ------------------------------ Task (JSON) ------------------------------ */

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,

    #[serde(default)]
    pub status: TaskStatus,

    pub title: String,

    #[serde(default)]
    pub priority: TaskPriority,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub scope: Vec<String>,

    #[serde(default)]
    pub evidence: Vec<String>,

    #[serde(default)]
    pub plan: Vec<String>,

    #[serde(default)]
    pub notes: Vec<String>,

    /// Original human request that created the task (Task Builder / Scan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,

    /// Optional per-task agent override (runner/model/model_effort/iterations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<TaskAgent>,

    /// RFC3339 UTC timestamps as strings to keep the contract tool-agnostic.
    #[schemars(required)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[schemars(required)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,

    /// RFC3339 UTC timestamp when work on this task actually started.
    ///
    /// Invariants:
    /// - Must be RFC3339 UTC (Z) if set.
    /// - Should be set when transitioning into `doing` (see status policy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,

    /// RFC3339 timestamp when the task should become runnable (optional scheduling).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_start: Option<String>,

    /// Task IDs that this task depends on (must be Done or Rejected before this task can run).
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Task IDs that this task blocks (must be Done/Rejected before blocked tasks can run).
    /// Semantically different from depends_on: blocks is "I prevent X" vs depends_on "I need X".
    #[serde(default)]
    pub blocks: Vec<String>,

    /// Task IDs that this task relates to (loose coupling, no execution constraint).
    /// Bidirectional awareness but no execution constraint.
    #[serde(default)]
    pub relates_to: Vec<String>,

    /// Task ID that this task duplicates (if any).
    /// Singular reference, not a list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicates: Option<String>,

    /// Custom user-defined fields (key-value pairs for extensibility).
    #[serde(default)]
    pub custom_fields: HashMap<String, String>,

    /// Parent task ID if this is a subtask (child-to-parent reference).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    #[default]
    Todo,
    Doing,
    Done,
    Rejected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Critical,
    High,
    #[default]
    Medium,
    Low,
}

// Custom PartialOrd implementation: Critical > High > Medium > Low
impl PartialOrd for TaskPriority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// Custom Ord implementation: Critical > High > Medium > Low (semantically)
// Higher priority = Greater in comparison, so Critical > High > Medium > Low
impl Ord for TaskPriority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare by weight: higher weight = higher priority = Greater
        self.weight().cmp(&other.weight())
    }
}

impl TaskPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskPriority::Critical => "critical",
            TaskPriority::High => "high",
            TaskPriority::Medium => "medium",
            TaskPriority::Low => "low",
        }
    }

    pub fn weight(self) -> u8 {
        match self {
            TaskPriority::Critical => 3,
            TaskPriority::High => 2,
            TaskPriority::Medium => 1,
            TaskPriority::Low => 0,
        }
    }

    /// Cycle to the next priority in ascending order, wrapping after Critical.
    pub fn cycle(self) -> Self {
        match self {
            TaskPriority::Low => TaskPriority::Medium,
            TaskPriority::Medium => TaskPriority::High,
            TaskPriority::High => TaskPriority::Critical,
            TaskPriority::Critical => TaskPriority::Low,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Draft => "draft",
            TaskStatus::Todo => "todo",
            TaskStatus::Doing => "doing",
            TaskStatus::Done => "done",
            TaskStatus::Rejected => "rejected",
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskAgent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<Runner>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,

    /// Per-task reasoning effort override for Codex models. Default falls back to config.
    #[serde(default, skip_serializing_if = "model_effort_is_default")]
    #[schemars(schema_with = "model_effort_schema")]
    pub model_effort: ModelEffort,

    /// Number of iterations to run for this task (overrides config).
    #[schemars(range(min = 1))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iterations: Option<u8>,

    /// Reasoning effort override for follow-up iterations (iterations > 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followup_reasoning_effort: Option<ReasoningEffort>,

    /// Optional normalized runner CLI overrides for this task.
    ///
    /// This is intended to express runner behavior intent (output/approval/sandbox/etc)
    /// without embedding runner-specific flag syntax into the queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_cli: Option<RunnerCliOptionsPatch>,
}

fn model_effort_is_default(value: &ModelEffort) -> bool {
    matches!(value, ModelEffort::Default)
}

fn model_effort_schema(
    generator: &mut schemars::r#gen::SchemaGenerator,
) -> schemars::schema::Schema {
    let mut schema = <ModelEffort as JsonSchema>::json_schema(generator);
    if let schemars::schema::Schema::Object(ref mut schema_object) = schema {
        schema_object.metadata().default = Some(json!("default"));
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::TaskPriority;

    #[test]
    fn task_priority_cycle_wraps_through_all_values() {
        assert_eq!(TaskPriority::Low.cycle(), TaskPriority::Medium);
        assert_eq!(TaskPriority::Medium.cycle(), TaskPriority::High);
        assert_eq!(TaskPriority::High.cycle(), TaskPriority::Critical);
        assert_eq!(TaskPriority::Critical.cycle(), TaskPriority::Low);
    }
}
