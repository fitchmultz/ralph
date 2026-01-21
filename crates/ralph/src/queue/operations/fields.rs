//! Custom field mutation helpers for queue tasks.

use super::validate::parse_rfc3339_utc;
use crate::contracts::QueueFile;
use anyhow::{anyhow, bail, Result};

pub fn set_field(
    queue: &mut QueueFile,
    task_id: &str,
    key: &str,
    value: &str,
    now_rfc3339: &str,
) -> Result<()> {
    let key_trimmed = key.trim();
    if key_trimmed.is_empty() {
        bail!("Missing custom field key: a key is required for this operation. Provide a valid key (e.g., 'severity').");
    }
    if key_trimmed.chars().any(|c| c.is_whitespace()) {
        bail!(
            "Invalid custom field key: '{}' contains whitespace. Custom field keys must not contain whitespace.",
            key_trimmed
        );
    }

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("Missing task_id: a task ID is required for this operation. Provide a valid ID (e.g., 'RQ-0001').");
    }

    let now = parse_rfc3339_utc(now_rfc3339)?;

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    task.custom_fields
        .insert(key_trimmed.to_string(), value.trim().to_string());
    task.updated_at = Some(now);

    Ok(())
}
