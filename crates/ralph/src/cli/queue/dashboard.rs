//! Queue dashboard subcommand - aggregated analytics for GUI clients.
//!
//! Responsibilities:
//! - Load queue, done, and productivity data once.
//! - Delegate to reports::dashboard for aggregation.
//! - Emit a single JSON payload combining all dashboard sections.
//!
//! Not handled here:
//! - Report data structures (see reports/dashboard.rs).
//! - Queue file loading (see crate::queue).
//!
//! Invariants/assumptions:
//! - Dashboard only supports JSON output (designed for GUI consumption).
//! - Productivity stats are optional (may not exist for new projects).

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::productivity;
use crate::reports;

/// Arguments for `ralph queue dashboard`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue dashboard\n  ralph queue dashboard --days 30\n  ralph queue dashboard --days 7\n\n\
The dashboard command returns all analytics data in a single JSON payload for GUI clients.\n\
Each section includes a 'status' field ('ok' or 'unavailable') for graceful partial failure handling."
)]
pub struct QueueDashboardArgs {
    /// Number of days for time-based analytics (velocity, burndown, history).
    #[arg(long, default_value_t = 30)]
    pub days: u32,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueDashboardArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Load productivity stats (optional - may not exist for new projects)
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let productivity_stats = productivity::load_productivity_stats(&cache_dir)
        .inspect_err(|e| log::debug!("Dashboard: productivity stats unavailable: {}", e))
        .ok();

    let report = reports::build_dashboard_report(
        &queue_file,
        done_ref,
        productivity_stats.as_ref(),
        args.days,
    );

    // Dashboard only supports JSON output (designed for GUI consumption)
    reports::print_dashboard(&report)?;

    Ok(())
}
