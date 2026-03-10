//! Task selection and date filtering for queue export.
//!
//! Responsibilities:
//! - Parse export date filters into comparable timestamps.
//! - Apply archive, status, tag, scope, ID, and created-at filters to queue tasks.
//! - Keep selection logic shared across export formats.
//!
//! Not handled here:
//! - Queue loading and warning emission.
//! - Output rendering for any export format.
//!
//! Invariants/assumptions:
//! - Date filters compare against `created_at`.
//! - Tasks missing `created_at` are excluded when date filters are active.

use anyhow::{Context, Result, bail};

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue;

use super::args::QueueExportArgs;

pub(super) fn parse_created_after(args: &QueueExportArgs) -> Result<Option<i64>> {
    args.created_after
        .as_deref()
        .map(parse_date_filter)
        .transpose()
}

pub(super) fn parse_created_before(args: &QueueExportArgs) -> Result<Option<i64>> {
    args.created_before
        .as_deref()
        .map(parse_date_filter)
        .transpose()
}

pub(super) fn collect_tasks<'a>(
    active: &'a QueueFile,
    done: Option<&'a QueueFile>,
    args: &QueueExportArgs,
    created_after: Option<i64>,
    created_before: Option<i64>,
) -> Vec<&'a Task> {
    let statuses: Vec<TaskStatus> = args.status.iter().copied().map(Into::into).collect();
    let mut tasks = Vec::new();

    if !args.only_archive {
        tasks.extend(queue::filter_tasks(
            active,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
    }

    if (args.include_archive || args.only_archive)
        && let Some(done_tasks) = done
    {
        tasks.extend(queue::filter_tasks(
            done_tasks,
            &statuses,
            &args.tag,
            &args.scope,
            None,
        ));
    }

    tasks
        .into_iter()
        .filter(|task| matches_id_pattern(task, args.id_pattern.as_deref()))
        .filter(|task| matches_created_filters(task, created_after, created_before))
        .collect()
}

pub(super) fn parse_date_filter(input: &str) -> Result<i64> {
    parse_timestamp(input).with_context(|| {
        format!(
            "Invalid date format: '{input}'. Expected RFC3339 (2026-01-15T00:00:00Z) or YYYY-MM-DD"
        )
    })
}

pub(super) fn parse_timestamp(input: &str) -> Result<i64> {
    if let Ok(dt) =
        time::OffsetDateTime::parse(input, &time::format_description::well_known::Rfc3339)
    {
        return Ok(dt.unix_timestamp());
    }

    let format = time::format_description::parse("[year]-[month]-[day]")
        .context("Failed to parse date format description")?;
    if let Ok(date) = time::Date::parse(input, &format) {
        return Ok(time::OffsetDateTime::new_utc(date, time::Time::MIDNIGHT).unix_timestamp());
    }

    bail!("Invalid timestamp format: '{input}'")
}

fn matches_id_pattern(task: &Task, id_pattern: Option<&str>) -> bool {
    id_pattern.is_none_or(|pattern| task.id.to_lowercase().contains(&pattern.to_lowercase()))
}

fn matches_created_filters(
    task: &Task,
    created_after: Option<i64>,
    created_before: Option<i64>,
) -> bool {
    timestamp_matches_after(task.created_at.as_deref(), created_after)
        && timestamp_matches_before(task.created_at.as_deref(), created_before)
}

fn timestamp_matches_after(created_at: Option<&str>, created_after: Option<i64>) -> bool {
    let Some(after) = created_after else {
        return true;
    };
    let Some(created_at) = created_at else {
        return false;
    };

    parse_timestamp(created_at).is_ok_and(|created_ts| created_ts >= after)
}

fn timestamp_matches_before(created_at: Option<&str>, created_before: Option<i64>) -> bool {
    let Some(before) = created_before else {
        return true;
    };
    let Some(created_at) = created_at else {
        return false;
    };

    parse_timestamp(created_at).is_ok_and(|created_ts| created_ts <= before)
}
