//! Shared diff helpers for task command reporting.
//!
//! Purpose:
//! - Compute field-level task JSON diffs for task update reporting.
//!
//! Responsibilities:
//! - Compare before/after JSON payloads and return a stable list of changed top-level fields.
//!
//! Not handled here:
//! - Logging.
//! - Queue loading.
//! - Semantic validation of task updates.
//!
//! Usage:
//! - Used by task update reporting helpers and tests that assert task field deltas.
//!
//! Invariants/assumptions:
//! - Inputs are valid JSON objects or valid JSON values; non-object comparisons collapse to `task`.
//! - Changed field ordering follows the after-object iteration order.

use anyhow::Result;

pub fn compare_task_fields(before: &str, after: &str) -> Result<Vec<String>> {
    let before_value: serde_json::Value = serde_json::from_str(before)?;
    let after_value: serde_json::Value = serde_json::from_str(after)?;

    if let (Some(before_obj), Some(after_obj)) = (before_value.as_object(), after_value.as_object())
    {
        let mut changed = Vec::new();
        for (key, after_val) in after_obj {
            if let Some(before_val) = before_obj.get(key) {
                if before_val != after_val {
                    changed.push(key.clone());
                }
            } else {
                changed.push(key.clone());
            }
        }
        Ok(changed)
    } else {
        Ok(vec!["task".to_string()])
    }
}
