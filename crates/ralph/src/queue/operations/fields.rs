//! Custom field mutation helpers for queue tasks.
//!
//! Responsibilities:
//! - Set or update a single custom field on a task.
//! - Validate task IDs, custom field keys, and timestamps.
//!
//! Does not handle:
//! - Persisting queue files to disk or resolving task selection rules.
//! - Validating non-custom task fields.
//!
//! Assumptions/invariants:
//! - Callers supply a loaded `QueueFile` and RFC3339 `now` timestamp.
//! - Custom field keys are non-empty and contain no whitespace.

use super::validate::{ensure_task_id, parse_rfc3339_utc, validate_custom_field_key};
use crate::contracts::QueueFile;
use anyhow::{Result, anyhow};

pub fn set_field(
    queue: &mut QueueFile,
    task_id: &str,
    key: &str,
    value: &str,
    now_rfc3339: &str,
) -> Result<()> {
    let operation = "custom field set";
    let key_trimmed = key.trim();
    let needle = ensure_task_id(task_id, operation)?;
    validate_custom_field_key(key_trimmed, needle, operation)?;

    let now = parse_rfc3339_utc(now_rfc3339)?;

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| {
            anyhow!(
                "Queue {} failed (task_id={}): task not found in .ralph/queue.json.",
                operation,
                needle
            )
        })?;

    task.custom_fields
        .insert(key_trimmed.to_string(), value.trim().to_string());
    task.updated_at = Some(now);

    Ok(())
}
