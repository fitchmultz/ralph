//! Input parsing helpers for task editing.
//!
//! Responsibilities:
//! - Parse status strings into TaskStatus values.
//! - Parse list inputs (comma or newline separated) into Vec<String>.
//! - Parse and normalize RFC3339 timestamp inputs.
//!
//! Does not handle:
//! - Full task validation (see `validate` module).
//! - Custom field parsing (see `validate::parse_custom_fields_with_context`).
//!
//! Assumptions/invariants:
//! - Input strings may contain whitespace that needs trimming.
//! - RFC3339 timestamps must be in UTC.

use crate::contracts::{TaskAgent, TaskStatus};
use crate::timeutil;
use anyhow::{Context, Result, bail};
use time::UtcOffset;

/// Parse a status string into TaskStatus.
pub(crate) fn parse_status(value: &str) -> Result<TaskStatus> {
    match value.trim().to_lowercase().as_str() {
        "draft" => Ok(TaskStatus::Draft),
        "todo" => Ok(TaskStatus::Todo),
        "doing" => Ok(TaskStatus::Doing),
        "done" => Ok(TaskStatus::Done),
        "rejected" => Ok(TaskStatus::Rejected),
        _ => bail!(
            "Invalid status: '{}'. Expected one of: draft, todo, doing, done, rejected.",
            value
        ),
    }
}

/// Parse a comma or newline separated list into a Vec of trimmed strings.
pub(crate) fn parse_list(input: &str) -> Vec<String> {
    input
        .split([',', '\n'])
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Parse and normalize an RFC3339 timestamp input.
///
/// Returns Ok(None) for empty input, Ok(Some(formatted)) for valid timestamps,
/// or an error if the timestamp is invalid or not in UTC.
pub(crate) fn normalize_rfc3339_input(label: &str, value: &str) -> Result<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let dt = timeutil::parse_rfc3339(trimmed)
        .with_context(|| format!("{} must be a valid RFC3339 timestamp", label))?;
    if dt.offset() != UtcOffset::UTC {
        bail!("{} must be a valid RFC3339 UTC timestamp", label);
    }
    let formatted = timeutil::format_rfc3339(dt)
        .with_context(|| format!("{} must be a valid RFC3339 timestamp", label))?;
    Ok(Some(formatted))
}

/// Parse a task agent override from JSON input.
///
/// Returns Ok(None) for empty input, or a parsed TaskAgent after validation.
pub(crate) fn parse_task_agent_override(input: &str) -> Result<Option<TaskAgent>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let parsed: TaskAgent = serde_json::from_str(trimmed)
        .context("agent must be a valid JSON object matching the task.agent contract")?;
    if let Some(iterations) = parsed.iterations
        && iterations == 0
    {
        bail!("agent.iterations must be >= 1");
    }
    if let Some(phases) = parsed.phases
        && !(1..=3).contains(&phases)
    {
        bail!("agent.phases must be one of: 1, 2, 3");
    }

    Ok(Some(parsed))
}

/// Cycle to the next status in the order: Draft -> Todo -> Doing -> Done -> Rejected -> Draft.
pub(crate) fn cycle_status(status: TaskStatus) -> TaskStatus {
    match status {
        TaskStatus::Draft => TaskStatus::Todo,
        TaskStatus::Todo => TaskStatus::Doing,
        TaskStatus::Doing => TaskStatus::Done,
        TaskStatus::Done => TaskStatus::Rejected,
        TaskStatus::Rejected => TaskStatus::Draft,
    }
}
