//! Task start handler for `ralph task start`.
//!
//! Purpose:
//! - Task start handler for `ralph task start`.
//!
//! Responsibilities:
//! - Set `started_at` for a task (RFC3339 UTC).
//! - Transition task status to `doing` when appropriate.
//!
//! Not handled here:
//! - Terminal completion/archiving (see `status.rs` and `queue::complete_task`).
//! - Batch operations (see `batch.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Queue file is locked during mutation.
//! - Status transition policy centralizes timestamp invariants (preferred).

use crate::cli::task::args::TaskStartArgs;
use crate::config;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::timeutil;
use anyhow::{Result, bail};

pub fn handle(args: &TaskStartArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task start", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(resolved, &format!("task start {}", args.task_id))?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    let task_id = args.task_id.clone();

    let task = queue_file
        .tasks
        .iter_mut()
        .find(|t| t.id == task_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue(&task_id)
            )
        })?;

    if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
        bail!(
            "Cannot start task {} because it is terminal (status: {}).",
            task.id,
            task.status
        );
    }

    if task.started_at.is_some() && !args.reset {
        println!(
            "Task {} already started at {}; no change.",
            task.id,
            task.started_at.as_deref().unwrap_or("")
        );
        return Ok(());
    }

    let task_id_for_msg = task.id.clone();
    task.started_at = Some(now.clone());

    if task.status != TaskStatus::Doing {
        queue::apply_status_policy(task, TaskStatus::Doing, &now, None)?;
    } else {
        task.updated_at = Some(now.clone());
    }

    queue::save_queue(&resolved.queue_path, &queue_file)?;
    println!("Started task {} (status: doing).", task_id_for_msg);
    Ok(())
}
