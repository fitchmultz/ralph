//! Queue-config validation rules.
//!
//! Purpose:
//! - Queue-config validation rules.
//!
//! Responsibilities:
//! - Validate queue override fields and threshold ranges.
//! - Enforce queue aging-threshold ordering rules.
//!
//! Not handled here:
//! - Active/done queue file contents.
//! - Agent or trust validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue overrides are optional but must be valid when specified.
//! - Aging thresholds must be strictly increasing when paired.

use crate::contracts::{QueueAgingThresholds, QueueConfig};
use anyhow::{Result, bail};
use std::path::Path;

pub const ERR_EMPTY_QUEUE_ID_PREFIX: &str = "Empty queue.id_prefix: prefix is required if specified. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.jsonc or via --id-prefix.";
pub const ERR_INVALID_QUEUE_ID_WIDTH: &str = "Invalid queue.id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.jsonc or via --id-width.";
pub const ERR_EMPTY_QUEUE_FILE: &str = "Empty queue.file: path is required if specified. Specify a valid path (e.g., '.ralph/queue.jsonc') in .ralph/config.jsonc or via --queue-file.";
pub const ERR_EMPTY_QUEUE_DONE_FILE: &str = "Empty queue.done_file: path is required if specified. Specify a valid path (e.g., '.ralph/done.jsonc') in .ralph/config.jsonc or via --done-file.";

pub fn validate_queue_id_prefix_override(id_prefix: Option<&str>) -> Result<()> {
    if let Some(prefix) = id_prefix
        && prefix.trim().is_empty()
    {
        bail!(ERR_EMPTY_QUEUE_ID_PREFIX);
    }
    Ok(())
}

pub fn validate_queue_id_width_override(id_width: Option<u8>) -> Result<()> {
    if let Some(width) = id_width
        && width == 0
    {
        bail!(ERR_INVALID_QUEUE_ID_WIDTH);
    }
    Ok(())
}

pub fn validate_queue_file_override(file: Option<&Path>) -> Result<()> {
    if let Some(path) = file
        && path.as_os_str().is_empty()
    {
        bail!(ERR_EMPTY_QUEUE_FILE);
    }
    Ok(())
}

pub fn validate_queue_done_file_override(done_file: Option<&Path>) -> Result<()> {
    if let Some(path) = done_file
        && path.as_os_str().is_empty()
    {
        bail!(ERR_EMPTY_QUEUE_DONE_FILE);
    }
    Ok(())
}

pub fn validate_queue_overrides(queue: &QueueConfig) -> Result<()> {
    validate_queue_id_prefix_override(queue.id_prefix.as_deref())?;
    validate_queue_id_width_override(queue.id_width)?;
    validate_queue_file_override(queue.file.as_deref())?;
    validate_queue_done_file_override(queue.done_file.as_deref())?;
    validate_queue_thresholds(queue)?;
    Ok(())
}

pub fn validate_queue_thresholds(queue: &QueueConfig) -> Result<()> {
    if let Some(threshold) = queue.size_warning_threshold_kb
        && !(100..=10000).contains(&threshold)
    {
        bail!(
            "Invalid queue.size_warning_threshold_kb: {}. Value must be between 100 and 10000 (inclusive). Update .ralph/config.jsonc.",
            threshold
        );
    }

    if let Some(threshold) = queue.task_count_warning_threshold
        && !(50..=5000).contains(&threshold)
    {
        bail!(
            "Invalid queue.task_count_warning_threshold: {}. Value must be between 50 and 5000 (inclusive). Update .ralph/config.jsonc.",
            threshold
        );
    }

    if let Some(depth) = queue.max_dependency_depth
        && !(1..=100).contains(&depth)
    {
        bail!(
            "Invalid queue.max_dependency_depth: {}. Value must be between 1 and 100 (inclusive). Update .ralph/config.jsonc.",
            depth
        );
    }

    if let Some(days) = queue.auto_archive_terminal_after_days
        && days > 3650
    {
        bail!(
            "Invalid queue.auto_archive_terminal_after_days: {}. Value must be between 0 and 3650 (inclusive). Update .ralph/config.jsonc.",
            days
        );
    }

    Ok(())
}

pub fn validate_queue_aging_thresholds(thresholds: &Option<QueueAgingThresholds>) -> Result<()> {
    let Some(thresholds) = thresholds else {
        return Ok(());
    };

    let warning = thresholds.warning_days;
    let stale = thresholds.stale_days;
    let rotten = thresholds.rotten_days;

    if let (Some(w), Some(s)) = (warning, stale)
        && w >= s
    {
        bail!(format_aging_threshold_error(Some(w), Some(s), rotten));
    }
    if let (Some(s), Some(r)) = (stale, rotten)
        && s >= r
    {
        bail!(format_aging_threshold_error(warning, Some(s), Some(r)));
    }
    if let (Some(w), Some(r)) = (warning, rotten)
        && w >= r
    {
        bail!(format_aging_threshold_error(Some(w), stale, Some(r)));
    }

    Ok(())
}

fn format_aging_threshold_error(
    warning: Option<u32>,
    stale: Option<u32>,
    rotten: Option<u32>,
) -> String {
    format!(
        "Invalid queue.aging_thresholds ordering: require warning_days < stale_days < rotten_days (got warning_days={}, stale_days={}, rotten_days={}). Update .ralph/config.jsonc.",
        warning
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unset".to_string()),
        stale
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unset".to_string()),
        rotten
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unset".to_string()),
    )
}
