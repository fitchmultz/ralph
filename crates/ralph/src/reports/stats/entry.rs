//! Stats report entrypoint orchestration.
//!
//! Purpose:
//! - Stats report entrypoint orchestration.
//!
//! Responsibilities:
//! - Attach optional execution-history ETA data to stats reports.
//! - Dispatch fully-built reports to text or JSON rendering.
//!
//! Not handled here:
//! - Stats metric calculation.
//! - CLI argument parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `build_stats_report` returns a complete report shape before ETA decoration.

use std::path::Path;

use anyhow::Result;

use crate::contracts::{AgentConfig, QueueFile};

use super::eta::build_execution_history_eta;
use super::report::build_stats_report;

pub(crate) fn print_stats(
    queue: &QueueFile,
    done: Option<&QueueFile>,
    tags: &[String],
    format: super::super::shared::ReportFormat,
    queue_file_size_kb: u64,
    config_agent: &AgentConfig,
    cache_dir: Option<&Path>,
) -> Result<()> {
    let mut report = build_stats_report(queue, done, tags);
    if let Some(cache_dir) = cache_dir {
        report.execution_history_eta = build_execution_history_eta(config_agent, cache_dir);
    }

    match format {
        super::super::shared::ReportFormat::Json => super::super::shared::print_json(&report)?,
        super::super::shared::ReportFormat::Text => {
            super::render::print_text_report(&report, queue_file_size_kb, cache_dir.is_some());
        }
    }

    Ok(())
}
