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
use serde::de::{self, Deserializer};
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

    /// Detailed description of the task's context, goal, purpose, and desired outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

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
    /// Values may be written as string/number/boolean; Ralph coerces them to strings when loading.
    #[serde(default, deserialize_with = "deserialize_custom_fields")]
    #[schemars(schema_with = "custom_fields_schema")]
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

/// Custom deserializer for `custom_fields` that coerces scalar values (string/number/bool)
/// to strings, while rejecting null, arrays, and objects with descriptive errors.
fn deserialize_custom_fields<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
    let raw = match value {
        serde_json::Value::Object(map) => map,
        serde_json::Value::Null => {
            return Err(de::Error::custom(
                "custom_fields must be an object (map); null is not allowed",
            ));
        }
        other => {
            return Err(de::Error::custom(format!(
                "custom_fields must be an object (map); got {}",
                other
            )));
        }
    };

    raw.into_iter()
        .map(|(k, v)| {
            let s = match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => {
                    return Err(de::Error::custom(format!(
                        "custom_fields['{}'] must be a string/number/boolean (null is not allowed)",
                        k
                    )));
                }
                serde_json::Value::Array(_) => {
                    return Err(de::Error::custom(format!(
                        "custom_fields['{}'] must be a scalar (string/number/boolean); arrays are not allowed",
                        k
                    )));
                }
                serde_json::Value::Object(_) => {
                    return Err(de::Error::custom(format!(
                        "custom_fields['{}'] must be a scalar (string/number/boolean); objects are not allowed",
                        k
                    )));
                }
            };
            Ok((k, s))
        })
        .collect()
}

/// Schema generator for `custom_fields` that accepts string/number/boolean values.
fn custom_fields_schema(
    _generator: &mut schemars::r#gen::SchemaGenerator,
) -> schemars::schema::Schema {
    use schemars::schema::{
        InstanceType, Metadata, ObjectValidation, Schema, SchemaObject, SingleOrVec,
        SubschemaValidation,
    };

    let scalar_any_of = SchemaObject {
        subschemas: Some(Box::new(SubschemaValidation {
            any_of: Some(vec![
                Schema::Object(SchemaObject {
                    instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
                    ..Default::default()
                }),
                Schema::Object(SchemaObject {
                    instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Number))),
                    ..Default::default()
                }),
                Schema::Object(SchemaObject {
                    instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Boolean))),
                    ..Default::default()
                }),
            ]),
            ..Default::default()
        })),
        ..Default::default()
    };

    let obj = SchemaObject {
        instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
        metadata: Some(Box::new(Metadata {
            description: Some(
                "Custom user-defined fields. Values may be written as string/number/boolean; Ralph coerces them to strings when loading the queue.".to_string(),
            ),
            ..Default::default()
        })),
        object: Some(Box::new(ObjectValidation {
            additional_properties: Some(Box::new(Schema::Object(scalar_any_of))),
            ..Default::default()
        })),
        ..Default::default()
    };

    Schema::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::{Task, TaskPriority};
    use std::collections::HashMap;

    #[test]
    fn task_priority_cycle_wraps_through_all_values() {
        assert_eq!(TaskPriority::Low.cycle(), TaskPriority::Medium);
        assert_eq!(TaskPriority::Medium.cycle(), TaskPriority::High);
        assert_eq!(TaskPriority::High.cycle(), TaskPriority::Critical);
        assert_eq!(TaskPriority::Critical.cycle(), TaskPriority::Low);
    }

    #[test]
    fn task_custom_fields_deserialize_coerces_scalars_to_strings() {
        let raw = r#"{
            "id": "RQ-0001",
            "title": "t",
            "custom_fields": {
                "guide_line_count": 1411,
                "enabled": true,
                "owner": "ralph"
            }
        }"#;

        let task: Task = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(
            task.custom_fields
                .get("guide_line_count")
                .map(String::as_str),
            Some("1411")
        );
        assert_eq!(
            task.custom_fields.get("enabled").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            task.custom_fields.get("owner").map(String::as_str),
            Some("ralph")
        );
    }

    #[test]
    fn task_custom_fields_deserialize_rejects_null() {
        let raw = r#"{"id":"RQ-0001","title":"t","custom_fields":{"x":null}}"#;
        let err = serde_json::from_str::<Task>(raw).unwrap_err();
        let err_msg = err.to_string().to_lowercase();
        assert!(
            err_msg.contains("custom_fields"),
            "error should mention custom_fields: {}",
            err_msg
        );
        assert!(
            err_msg.contains("null"),
            "error should mention null: {}",
            err_msg
        );
    }

    #[test]
    fn task_custom_fields_deserialize_rejects_custom_fields_null() {
        let raw = r#"{"id":"RQ-0001","title":"t","custom_fields":null}"#;
        let err = serde_json::from_str::<Task>(raw).unwrap_err();
        let err_msg = err.to_string().to_lowercase();
        assert!(
            err_msg.contains("custom_fields"),
            "error should mention custom_fields: {}",
            err_msg
        );
        assert!(
            err_msg.contains("null"),
            "error should mention null: {}",
            err_msg
        );
    }

    #[test]
    fn task_custom_fields_deserialize_rejects_custom_fields_non_object() {
        let raw = r#"{"id":"RQ-0001","title":"t","custom_fields":123}"#;
        let err = serde_json::from_str::<Task>(raw).unwrap_err();
        let err_msg = err.to_string().to_lowercase();
        assert!(
            err_msg.contains("custom_fields"),
            "error should mention custom_fields: {}",
            err_msg
        );
        assert!(
            err_msg.contains("object") || err_msg.contains("map"),
            "error should mention object/map: {}",
            err_msg
        );
    }

    #[test]
    fn task_custom_fields_deserialize_rejects_object_and_array_values() {
        let raw_obj = r#"{"id":"RQ-0001","title":"t","custom_fields":{"x":{"a":1}}}"#;
        let raw_arr = r#"{"id":"RQ-0001","title":"t","custom_fields":{"x":[1,2]}}"#;

        let err_obj = serde_json::from_str::<Task>(raw_obj).unwrap_err();
        let err_arr = serde_json::from_str::<Task>(raw_arr).unwrap_err();

        let err_obj_msg = err_obj.to_string().to_lowercase();
        let err_arr_msg = err_arr.to_string().to_lowercase();

        assert!(
            err_obj_msg.contains("custom_fields"),
            "object error should mention custom_fields: {}",
            err_obj_msg
        );
        assert!(
            err_arr_msg.contains("custom_fields"),
            "array error should mention custom_fields: {}",
            err_arr_msg
        );
    }

    #[test]
    fn task_custom_fields_serializes_as_strings() {
        let mut custom_fields = HashMap::new();
        custom_fields.insert("count".to_string(), "42".to_string());
        custom_fields.insert("enabled".to_string(), "true".to_string());

        let task = Task {
            id: "RQ-0001".to_string(),
            title: "Test".to_string(),
            custom_fields,
            ..Default::default()
        };

        let json = serde_json::to_string(&task).expect("serialize");
        assert!(json.contains("\"count\":\"42\""));
        assert!(json.contains("\"enabled\":\"true\""));
    }
}
