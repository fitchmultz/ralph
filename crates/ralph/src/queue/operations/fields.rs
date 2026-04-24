//! Custom field mutation helpers for queue tasks.
//!
//! Purpose:
//! - Custom field mutation helpers for queue tasks.
//!
//! Responsibilities:
//! - Set or update a single custom field on a task.
//! - Validate task IDs, custom field keys, and timestamps.
//! - Preview custom field changes without applying them.
//!
//! Non-scope:
//! - Persisting queue files to disk or resolving task selection rules.
//! - Validating non-custom task fields.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Callers supply a loaded `QueueFile` and RFC3339 `now` timestamp.
//! - Custom field keys are non-empty and contain no whitespace.

use super::validate::{ensure_task_id, parse_rfc3339_utc, validate_custom_field_key};
use crate::contracts::QueueFile;
use anyhow::{Result, anyhow};

/// Preview of what would change in a custom field set operation.
#[derive(Debug, Clone)]
pub struct TaskFieldPreview {
    pub task_id: String,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: String,
}

/// Preview custom field changes without applying them.
pub fn preview_set_field(
    queue: &QueueFile,
    task_id: &str,
    key: &str,
    value: &str,
) -> Result<TaskFieldPreview> {
    let operation = "preview field set";
    let key_trimmed = key.trim();
    let needle = ensure_task_id(task_id, operation)?;
    validate_custom_field_key(key_trimmed, needle, operation)?;

    let task = queue
        .tasks
        .iter()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| {
            anyhow!(
                "{}",
                crate::error_messages::task_not_found_for_edit(operation, needle)
            )
        })?;

    let old_value = task.custom_fields.get(key_trimmed).cloned();
    let new_value = value.trim().to_string();

    Ok(TaskFieldPreview {
        task_id: needle.to_string(),
        key: key_trimmed.to_string(),
        old_value,
        new_value,
    })
}

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
                "{}",
                crate::error_messages::task_not_found_for_edit(operation, needle)
            )
        })?;

    task.custom_fields
        .insert(key_trimmed.to_string(), value.trim().to_string());
    task.updated_at = Some(now);

    Ok(())
}
