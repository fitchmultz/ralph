//! Queue next subcommand.

use anyhow::Result;
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::{outpututil, queue};

/// Arguments for `ralph queue next`.
#[derive(Args)]
#[command(after_long_help = "Example:\n  ralph queue next --with-title")]
pub struct QueueNextArgs {
    /// Include the task title after the ID.
    #[arg(long)]
    pub with_title: bool,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueNextArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    if let Some(next) = queue::next_runnable_task(&queue_file, done_ref) {
        if args.with_title {
            println!(
                "{}",
                outpututil::format_task_id_title(&next.id, &next.title)
            );
        } else {
            println!("{}", outpututil::format_task_id(&next.id));
        }
        return Ok(());
    }

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let next_id = queue::next_id_across(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    println!("{next_id}");
    Ok(())
}
