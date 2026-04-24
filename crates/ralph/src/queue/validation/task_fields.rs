//! Queue task field validation.
//!
//! Purpose:
//! - Queue task field validation.
//!
//! Responsibilities:
//! - Validate per-task required fields, timestamps, IDs, and agent limits.
//! - Keep task-level validation separate from graph-level reasoning.
//!
//! Not handled here:
//! - Cross-task relationship validation.
//! - Queue-set duplicate detection.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Timestamp validation requires RFC3339 UTC values.
//! - Queue ID format remains `PREFIX-NNNN`.

use crate::contracts::{Task, TaskStatus};
use crate::timeutil;
use anyhow::{Context, Result, anyhow, bail};
use time::UtcOffset;

pub(crate) fn validate_task_required_fields(index: usize, task: &Task) -> Result<()> {
    if task.id.trim().is_empty() {
        bail!(
            "Missing task ID: task at index {} is missing an 'id' field. Add a valid ID (e.g., 'RQ-0001') to the task.",
            index
        );
    }
    if task.title.trim().is_empty() {
        bail!(
            "Missing task title: task {} (index {}) is missing a 'title' field. Add a descriptive title (e.g., 'Fix login bug').",
            task.id,
            index
        );
    }
    ensure_list_valid("tags", index, &task.id, &task.tags)?;
    ensure_list_valid("scope", index, &task.id, &task.scope)?;
    ensure_list_valid("evidence", index, &task.id, &task.evidence)?;
    ensure_list_valid("plan", index, &task.id, &task.plan)?;

    for (key_idx, (key, _value)) in task.custom_fields.iter().enumerate() {
        let key_trimmed = key.trim();
        if key_trimmed.is_empty() {
            bail!(
                "Empty custom field key: task {} (index {}) has an empty key at custom_fields[{}]. Remove the empty key or provide a valid key.",
                task.id,
                index,
                key_idx
            );
        }
        if key_trimmed.chars().any(|c| c.is_whitespace()) {
            bail!(
                "Invalid custom field key: task {} (index {}) has a key with whitespace at custom_fields[{}]: '{}'. Custom field keys must not contain whitespace.",
                task.id,
                index,
                key_idx,
                key_trimmed
            );
        }
    }

    if let Some(ts) = task.created_at.as_deref() {
        validate_rfc3339("created_at", index, &task.id, ts)?;
    } else {
        bail!(
            "Missing created_at: task {} (index {}) is missing the 'created_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13.000000000Z').",
            task.id,
            index
        );
    }

    if let Some(ts) = task.updated_at.as_deref() {
        validate_rfc3339("updated_at", index, &task.id, ts)?;
    } else {
        bail!(
            "Missing updated_at: task {} (index {}) is missing the 'updated_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13.000000000Z').",
            task.id,
            index
        );
    }

    if let Some(ts) = task.completed_at.as_deref() {
        validate_rfc3339("completed_at", index, &task.id, ts)?;
    } else if task.status == TaskStatus::Done || task.status == TaskStatus::Rejected {
        bail!(
            "Missing completed_at: task {} (index {}) is in status '{:?}' but missing 'completed_at'. Add a valid RFC3339 timestamp.",
            task.id,
            index,
            task.status
        );
    }

    Ok(())
}

pub(crate) fn validate_task_agent_fields(index: usize, task: &Task) -> Result<()> {
    if let Some(agent) = task.agent.as_ref() {
        if let Some(iterations) = agent.iterations
            && iterations == 0
        {
            bail!(
                "Invalid agent.iterations: task {} (index {}) must specify iterations >= 1.",
                task.id,
                index
            );
        }

        if let Some(phases) = agent.phases
            && !(1..=3).contains(&phases)
        {
            bail!(
                "Invalid agent.phases: task {} (index {}) must specify phases in [1, 2, 3].",
                task.id,
                index
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_task_id(
    index: usize,
    raw_id: &str,
    expected_prefix: &str,
    id_width: usize,
) -> Result<u32> {
    let trimmed = raw_id.trim();
    let (prefix_raw, num_raw) = trimmed.split_once('-').ok_or_else(|| {
        anyhow!(
            "Invalid task ID format: task at index {} has ID '{}' which is missing a '-'. Task IDs must follow the 'PREFIX-NUMBER' format (e.g., '{}-0001').",
            index,
            trimmed,
            expected_prefix
        )
    })?;

    let prefix = prefix_raw.trim().to_uppercase();
    if prefix != expected_prefix {
        bail!(
            "Mismatched task ID prefix: task at index {} has prefix '{}' but expected '{}'. Update the task ID to '{}' or change the prefix in .ralph/config.jsonc.",
            index,
            prefix,
            expected_prefix,
            super::super::format_id(expected_prefix, 1, id_width)
        );
    }

    let num = num_raw.trim();
    if num.len() != id_width {
        bail!(
            "Invalid task ID width: task at index {} has a numeric suffix of length {} but expected {}. Pad the numeric part with leading zeros (e.g., '{}').",
            index,
            num.len(),
            id_width,
            super::super::format_id(expected_prefix, num.parse().unwrap_or(1), id_width)
        );
    }
    if !num.chars().all(|c| c.is_ascii_digit()) {
        bail!(
            "Invalid task ID: task at index {} has non-digit characters in its numeric suffix '{}'. Ensure the ID suffix contains only digits (e.g., '0001').",
            index,
            num
        );
    }

    let value: u32 = num.parse().with_context(|| {
        format!(
            "task[{}] id numeric suffix must parse as integer (got: {})",
            index, num
        )
    })?;
    Ok(value)
}

fn validate_rfc3339(label: &str, index: usize, id: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!(
            "Missing {}: task {} (index {}) requires a non-empty '{}' field. Add a valid RFC3339 UTC timestamp (e.g., '2026-01-19T05:23:13.000000000Z').",
            label,
            id,
            index,
            label
        );
    }
    let dt = timeutil::parse_rfc3339(trimmed).with_context(|| {
        format!(
            "task[{}] {} must be a valid RFC3339 UTC timestamp (got: {}, id={}). Example: '2026-01-19T05:23:13.000000000Z'.",
            index, label, trimmed, id
        )
    })?;
    if dt.offset() != UtcOffset::UTC {
        bail!(
            "task[{}] {} must be a valid RFC3339 UTC timestamp (got: {}, id={}). Example: '2026-01-19T05:23:13.000000000Z'.",
            index,
            label,
            trimmed,
            id
        );
    }
    Ok(())
}

fn ensure_list_valid(label: &str, index: usize, id: &str, values: &[String]) -> Result<()> {
    for (i, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            bail!(
                "Empty {} item: task {} (index {}) contains an empty string at {}[{}]. Remove the empty item or add content.",
                label,
                id,
                index,
                label,
                i
            );
        }
    }
    Ok(())
}
