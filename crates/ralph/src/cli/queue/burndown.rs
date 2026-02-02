//! Queue burndown subcommand.

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::reports;

use super::QueueReportFormat;

/// Arguments for `ralph queue burndown`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue burndown\n  ralph queue burndown --days 30\n  ralph queue burndown --format json"
)]
pub struct QueueBurndownArgs {
    /// Number of days to show (default: 7).
    #[arg(long, default_value_t = 7)]
    pub days: u32,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueReportFormat::Text)]
    pub format: QueueReportFormat,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueBurndownArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());
    reports::print_burndown(&queue_file, done_ref, args.days, args.format.into())?;
    Ok(())
}
