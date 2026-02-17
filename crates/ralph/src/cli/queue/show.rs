//! Queue show subcommand.
//!
//! Responsibilities:
//! - Define CLI arguments for displaying a single task.
//! - Load queue + done data and render a task in JSON or compact form.
//!
//! Not handled here:
//! - Queue persistence beyond reading files.
//! - Validation or mutation beyond reading.
//! - Any task editing behavior.
//!
//! Invariants/assumptions:
//! - Queue files are valid and already validated by the loader.
//! - Task IDs are matched after trimming whitespace.

use anyhow::{Result, anyhow};
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::{outpututil, queue};

use super::QueueShowFormat;

/// Arguments for `ralph queue show`.
#[derive(Args)]
#[command(
    after_long_help = "Examples:\n  ralph queue show RQ-0001\n  ralph queue show RQ-0001 --format compact"
)]
pub struct QueueShowArgs {
    /// Task ID to show.
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Output format.
    #[arg(long, value_enum, default_value_t = QueueShowFormat::Json)]
    pub format: QueueShowFormat,
}

pub(crate) fn show_task(resolved: &Resolved, task_id: &str, format: QueueShowFormat) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    let task = queue::find_task_across(&queue_file, done_ref, task_id)
        .ok_or_else(|| anyhow!("{}", crate::error_messages::task_not_found(task_id.trim())))?;

    match format {
        QueueShowFormat::Json => {
            let rendered = serde_json::to_string_pretty(task)?;
            print!("{rendered}");
        }
        QueueShowFormat::Compact => {
            println!("{}", outpututil::format_task_compact(task));
        }
    }
    Ok(())
}

pub(crate) fn handle(resolved: &Resolved, args: QueueShowArgs) -> Result<()> {
    show_task(resolved, &args.task_id, args.format)
}
