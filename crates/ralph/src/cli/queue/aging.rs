//! Queue aging subcommand.
//!
//! Responsibilities:
//! - Handle `ralph queue aging` command to show task aging buckets.
//!
//! Not handled here:
//! - Actual aging computation (see crate::reports::aging).
//! - Output formatting.
//!
//! Invariants/assumptions:
//! - Queue files are validated before processing.

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues_read_only;
use crate::config::Resolved;
use crate::contracts::TaskStatus;
use crate::reports;

use super::{QueueReportFormat, StatusArg};

/// Arguments for `ralph queue aging`.
#[derive(Args)]
#[command(
    about = "Show task aging buckets to identify stale work",
    after_long_help = "Examples:\n  ralph queue aging\n  ralph queue aging --format json\n  ralph queue aging --status todo --status doing\n  ralph queue aging --status doing"
)]
pub struct QueueAgingArgs {
    /// Filter by status (repeatable). Default: todo, doing.
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueReportFormat::Text)]
    pub format: QueueReportFormat,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueAgingArgs) -> Result<()> {
    let (queue_file, _done_file) = load_and_validate_queues_read_only(resolved, false)?;

    let statuses: Vec<TaskStatus> = if args.status.is_empty() {
        vec![TaskStatus::Todo, TaskStatus::Doing]
    } else {
        args.status.into_iter().map(Into::into).collect()
    };

    let thresholds = reports::AgingThresholds::from_queue_config(&resolved.config.queue)?;
    reports::print_aging(&queue_file, &statuses, thresholds, args.format.into())?;
    Ok(())
}
