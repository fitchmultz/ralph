//! Task scheduling command handler for `ralph task schedule` subcommand.
//!
//! Responsibilities:
//! - Handle `schedule` command (set scheduled start time).
//! - Parse relative time expressions.
//! - Support `--clear` to remove scheduling.
//!
//! Not handled here:
//! - Task building or status changes (see `build.rs`, `status.rs`).
//! - Batch operations (see `batch.rs`).
//!
//! Invariants/assumptions:
//! - Supports RFC3339 timestamps and relative time expressions.
//! - Relative times are parsed via `timeutil::parse_relative_time`.
//! - Clear operation sets an empty string to remove the scheduled start.

use anyhow::{Result, bail};

use crate::cli::task::args::TaskScheduleArgs;
use crate::config;
use crate::queue;
use crate::queue::TaskEditKey;
use crate::timeutil;

/// Handle the `schedule` command.
pub fn handle(args: &TaskScheduleArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task schedule", force)?;
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Handle clear operation
    let value = if args.clear {
        String::new()
    } else if let Some(ref when) = args.when {
        // Parse relative time or RFC3339
        timeutil::parse_relative_time(when)?
    } else {
        bail!("Either provide a timestamp/expression or use --clear to remove scheduling.");
    };

    queue::apply_task_edit(
        &mut queue_file,
        None,
        &args.task_id,
        TaskEditKey::ScheduledStart,
        &value,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    queue::save_queue(&resolved.queue_path, &queue_file)?;

    if args.clear {
        log::info!("Task {} scheduling cleared.", args.task_id);
        println!("Task {} scheduling cleared.", args.task_id);
    } else {
        log::info!("Task {} scheduled for {}.", args.task_id, value);
        println!("Task {} scheduled for {}.", args.task_id, value);
    }

    Ok(())
}
