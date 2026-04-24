//! Aging report entrypoint orchestration.
//!
//! Purpose:
//! - Aging report entrypoint orchestration.
//!
//! Responsibilities:
//! - Capture the current timestamp for an aging report run.
//! - Route the built report to text or JSON rendering.
//!
//! Not handled here:
//! - Bucket computation or threshold validation.
//! - CLI argument parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Aging report assembly is deterministic for a given `now` timestamp.

use anyhow::Result;
use time::OffsetDateTime;

use crate::contracts::{QueueFile, TaskStatus};

use super::report::build_aging_report;
use super::thresholds::AgingThresholds;

pub(crate) fn print_aging(
    queue: &QueueFile,
    statuses: &[TaskStatus],
    thresholds: AgingThresholds,
    format: super::super::shared::ReportFormat,
) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    let report = build_aging_report(queue, statuses, thresholds, now);

    match format {
        super::super::shared::ReportFormat::Json => super::super::shared::print_json(&report)?,
        super::super::shared::ReportFormat::Text => super::render::print_text_report(&report),
    }

    Ok(())
}
