//! Queue history subcommand.

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues_read_only;
use crate::config::Resolved;
use crate::reports;

use super::QueueReportFormat;

/// Arguments for `ralph queue history`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue history\n  ralph queue history --days 14\n  ralph queue history --format json"
)]
pub struct QueueHistoryArgs {
    /// Number of days to show (default: 7).
    #[arg(long, default_value_t = 7)]
    pub days: u32,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueReportFormat::Text)]
    pub format: QueueReportFormat,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueHistoryArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues_read_only(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
    reports::print_history(&queue_file, done_ref, args.days, args.format.into())?;
    Ok(())
}
