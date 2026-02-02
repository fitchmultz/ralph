//! Queue stats subcommand.

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::queue;
use crate::reports;

use super::QueueReportFormat;

/// Arguments for `ralph queue stats`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue stats\n  ralph queue stats --tag rust --tag cli\n  ralph queue stats --format json"
)]
pub struct QueueStatsArgs {
    /// Filter by tag (repeatable, case-insensitive).
    #[arg(long)]
    pub tag: Vec<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueReportFormat::Text)]
    pub format: QueueReportFormat,

    /// Suppress size warning output.
    #[arg(long, short)]
    pub quiet: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueStatsArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;

    // Check queue size and print warning if needed
    if !args.quiet {
        let size_threshold =
            queue::size_threshold_or_default(resolved.config.queue.size_warning_threshold_kb);
        let count_threshold =
            queue::count_threshold_or_default(resolved.config.queue.task_count_warning_threshold);
        if let Ok(result) = queue::check_queue_size(
            &resolved.queue_path,
            queue_file.tasks.len(),
            size_threshold,
            count_threshold,
        ) {
            queue::print_size_warning_if_needed(&result, args.quiet);
        }
    }

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Get file size for display
    let file_size_kb = std::fs::metadata(&resolved.queue_path)
        .map(|m| m.len() / 1024)
        .unwrap_or(0);

    reports::print_stats(
        &queue_file,
        done_ref,
        &args.tag,
        args.format.into(),
        file_size_kb,
    )?;
    Ok(())
}
