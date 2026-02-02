//! Shared validation helpers for queue operations.
//!
//! Responsibilities:
//! - Validate common queue inputs (task IDs, custom fields, timestamps).
//! - Provide consistent, actionable error context for queue operations.
//!
//! Does not handle:
//! - Reading or writing queue files.
//! - Validating full task schemas (see queue schema validation).
//!
//! Assumptions/invariants:
//! - Callers pass raw user input that may need trimming.
//! - Errors are surfaced directly to CLI/TUI callers.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use time::UtcOffset;

use crate::timeutil;

pub(crate) fn parse_rfc3339_utc(now_rfc3339: &str) -> Result<String> {
    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!(
            "Missing timestamp: current time is required for this operation. Ensure a valid RFC3339 timestamp is provided."
        );
    }
    let dt = timeutil::parse_rfc3339(now).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;
    if dt.offset() != UtcOffset::UTC {
        bail!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        );
    }
    timeutil::format_rfc3339(dt).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })
}

pub(crate) fn ensure_task_id<'a>(task_id: &'a str, operation: &str) -> Result<&'a str> {
    let trimmed = task_id.trim();
    if trimmed.is_empty() {
        bail!(
            "Queue {} failed: missing task_id. Provide a valid ID (e.g., 'RQ-0001').",
            operation
        );
    }
    Ok(trimmed)
}

pub(crate) fn validate_custom_field_key(key: &str, task_id: &str, operation: &str) -> Result<()> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        bail!(
            "Queue {} failed (task_id={}, field=custom_fields): custom field key cannot be empty. Provide key=value (e.g., severity=high).",
            operation,
            task_id
        );
    }
    if trimmed.chars().any(|c| c.is_whitespace()) {
        bail!(
            "Queue {} failed (task_id={}, field=custom_fields): invalid key '{}' contains whitespace. Custom field keys must not contain whitespace.",
            operation,
            task_id,
            trimmed
        );
    }
    Ok(())
}

pub(crate) fn parse_custom_fields_with_context(
    task_id: &str,
    input: &str,
    operation: &str,
) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    if input.trim().is_empty() {
        return Ok(map);
    }

    for raw in input.split([',', '\n']) {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (key, value) = trimmed.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "Queue {} failed (task_id={}, field=custom_fields): invalid entry '{}'. Expected key=value (e.g., severity=high).",
                operation,
                task_id,
                trimmed
            )
        })?;
        validate_custom_field_key(key, task_id, operation)?;
        let value = value.trim();
        map.insert(key.trim().to_string(), value.to_string());
    }
    Ok(map)
}
