//! Purpose: Hold serde and schemars helper hooks for task contracts.
//!
//! Responsibilities:
//! - Provide skip-serialization/schema helpers for `ModelEffort` defaults.
//! - Provide `custom_fields` deserialization and schema hooks.
//!
//! Scope:
//! - Helper hooks only; task/task-agent data models and priority behavior live
//!   in sibling modules.
//!
//! Usage:
//! - Referenced by serde/schemars attributes in `contracts/task/types.rs`.
//!
//! Invariants/Assumptions:
//! - `custom_fields` accepts only scalar JSON values and coerces them to
//!   strings.
//! - The generated schema continues to advertise string/number/boolean custom
//!   field values.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use serde_json::json;

use crate::contracts::ModelEffort;

pub(super) fn model_effort_is_default(value: &ModelEffort) -> bool {
    matches!(value, ModelEffort::Default)
}

pub(super) fn model_effort_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut schema = <ModelEffort as JsonSchema>::json_schema(generator);
    schema
        .ensure_object()
        .insert("default".to_string(), json!("default"));
    schema
}

/// Custom deserializer for `custom_fields` that coerces scalar values
/// (string/number/bool) to strings, while rejecting null, arrays, and objects
/// with descriptive errors.
pub(super) fn deserialize_custom_fields<'de, D>(
    deserializer: D,
) -> std::result::Result<HashMap<String, String>, D::Error>
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
        .map(|(key, value)| {
            let scalar = match value {
                serde_json::Value::String(string) => string,
                serde_json::Value::Number(number) => number.to_string(),
                serde_json::Value::Bool(boolean) => boolean.to_string(),
                serde_json::Value::Null => {
                    return Err(de::Error::custom(format!(
                        "custom_fields['{}'] must be a string/number/boolean (null is not allowed)",
                        key
                    )));
                }
                serde_json::Value::Array(_) => {
                    return Err(de::Error::custom(format!(
                        "custom_fields['{}'] must be a scalar (string/number/boolean); arrays are not allowed",
                        key
                    )));
                }
                serde_json::Value::Object(_) => {
                    return Err(de::Error::custom(format!(
                        "custom_fields['{}'] must be a scalar (string/number/boolean); objects are not allowed",
                        key
                    )));
                }
            };
            Ok((key, scalar))
        })
        .collect()
}

/// Schema generator for `custom_fields` that accepts string/number/boolean
/// values.
pub(super) fn custom_fields_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "Custom user-defined fields. Values may be written as string/number/boolean; Ralph coerces them to strings when loading the queue.",
        "additionalProperties": {
            "anyOf": [
                {"type": "string"},
                {"type": "number"},
                {"type": "boolean"}
            ]
        }
    })
}
