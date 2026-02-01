//! Task status command handlers for `ralph task` subcommands.
//!
//! Responsibilities:
//! - Handle `ready` command (promote draft to todo).
//! - Handle `status` command (update task status).
//! - Handle `done` and `reject` commands (terminal status with archiving).
//!
//! Not handled here:
//! - Batch status operations (see `batch.rs`).
//! - Task building or editing (see `build.rs`, `edit.rs`).
//!
//! Invariants/assumptions:
//! - Terminal statuses (done, rejected) archive tasks to done.json.
//! - Non-terminal statuses update in-place in queue.json.
//! - Uses completion signal pattern when running under supervision.

use anyhow::{Result, bail};

use crate::cli::task::args::{TaskDoneArgs, TaskReadyArgs, TaskRejectArgs, TaskStatusArgs};
use crate::completions;
use crate::config;
use crate::contracts::TaskStatus;
use crate::lock;
use crate::queue;
use crate::timeutil;
use crate::webhook;

/// Handle the `ready` command (promote draft to todo).
pub fn handle_ready(args: &TaskReadyArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task ready", force)?;
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;

    // Get task info before modification for webhook
    let task_info = queue_file
        .tasks
        .iter()
        .find(|t| t.id == args.task_id)
        .map(|t| (t.id.clone(), t.title.clone()));

    queue::promote_draft_to_todo(&mut queue_file, &args.task_id, &now, args.note.as_deref())?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    // Trigger webhook for status change
    if let Some((task_id, task_title)) = task_info {
        webhook::notify_status_changed(
            &task_id,
            &task_title,
            "draft",
            "todo",
            &resolved.config.agent.webhook,
            &now,
        );
    }

    log::info!("Task {} marked ready (draft -> todo).", args.task_id);
    Ok(())
}

/// Handle the `status` command (update task status).
pub fn handle_status(
    args: &TaskStatusArgs,
    force: bool,
    resolved: &config::Resolved,
) -> Result<()> {
    let status: TaskStatus = args.status.into();

    match status {
        TaskStatus::Done | TaskStatus::Rejected => {
            // For terminal statuses, we need to handle each task individually
            // because complete_task involves moving tasks to done.json
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task status", force)?;
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

            // Resolve task IDs from explicit list or tag filter
            let task_ids =
                queue::operations::resolve_task_ids(&queue_file, &args.task_ids, &args.tag_filter)?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            let mut results = Vec::new();
            let mut succeeded = 0;
            let mut failed = 0;

            for task_id in &task_ids {
                match queue::complete_task(
                    &resolved.queue_path,
                    &resolved.done_path,
                    task_id,
                    status,
                    &now,
                    args.note
                        .as_deref()
                        .map(|n| vec![n.to_string()])
                        .as_deref()
                        .unwrap_or(&[]),
                    &resolved.id_prefix,
                    resolved.id_width,
                    max_depth,
                ) {
                    Ok(()) => {
                        results.push((task_id.clone(), true, None));
                        succeeded += 1;
                    }
                    Err(e) => {
                        results.push((task_id.clone(), false, Some(e.to_string())));
                        failed += 1;
                    }
                }
            }

            // Print results
            if failed > 0 {
                println!("Completed with errors:");
                for (task_id, success, error) in &results {
                    if *success {
                        println!("  ✓ {}: {} and archived", task_id, status);
                    } else {
                        println!(
                            "  ✗ {}: failed - {}",
                            task_id,
                            error.as_deref().unwrap_or("unknown error")
                        );
                    }
                }
                println!(
                    "Completed: {}/{} tasks {} successfully.",
                    succeeded,
                    task_ids.len(),
                    status
                );
            } else {
                println!("Successfully marked {} tasks as {}:", succeeded, status);
                for (task_id, _, _) in &results {
                    println!("  ✓ {}", task_id);
                }
            }

            Ok(())
        }
        TaskStatus::Draft | TaskStatus::Todo | TaskStatus::Doing => {
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task status", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;

            // Resolve task IDs from explicit list or tag filter
            let task_ids =
                queue::operations::resolve_task_ids(&queue_file, &args.task_ids, &args.tag_filter)?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            let result = queue::operations::batch_set_status(
                &mut queue_file,
                &task_ids,
                status,
                &now,
                args.note.as_deref(),
                false, // continue_on_error - default to atomic for CLI
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(
                &result,
                &format!("Status update to {}", status),
                false,
            );

            Ok(())
        }
    }
}

/// Handle the `done` command.
pub fn handle_done(args: &TaskDoneArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    // Get task info before completion for webhook
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let task_title = queue_file
        .tasks
        .iter()
        .find(|t| t.id == args.task_id)
        .map(|t| t.title.clone())
        .unwrap_or_default();

    complete_task_or_signal(
        resolved,
        &args.task_id,
        TaskStatus::Done,
        &args.note,
        force,
        "task done",
    )?;

    // Trigger webhook after successful completion
    let now = timeutil::now_utc_rfc3339()?;
    webhook::notify_task_completed(
        &args.task_id,
        &task_title,
        &resolved.config.agent.webhook,
        &now,
    );

    Ok(())
}

/// Handle the `reject` command.
pub fn handle_reject(
    args: &TaskRejectArgs,
    force: bool,
    resolved: &config::Resolved,
) -> Result<()> {
    // Get task info before completion for webhook
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let task_title = queue_file
        .tasks
        .iter()
        .find(|t| t.id == args.task_id)
        .map(|t| t.title.clone())
        .unwrap_or_default();

    let note_str = args.note.first().map(|s| s.as_str()).unwrap_or("");
    complete_task_or_signal(
        resolved,
        &args.task_id,
        TaskStatus::Rejected,
        &args.note,
        force,
        "task reject",
    )?;

    // Trigger webhook after successful rejection
    let now = timeutil::now_utc_rfc3339()?;
    webhook::notify_task_failed(
        &args.task_id,
        &task_title,
        Some(note_str),
        &resolved.config.agent.webhook,
        &now,
    );

    Ok(())
}

/// Complete a task or write a completion signal if under supervision.
fn complete_task_or_signal(
    resolved: &config::Resolved,
    task_id: &str,
    status: TaskStatus,
    notes: &[String],
    force: bool,
    _lock_label: &str,
) -> Result<()> {
    let lock_dir = lock::queue_lock_dir(&resolved.repo_root);
    // Only use completion signal mode if the current process is actually being supervised
    // (i.e., running as a descendant of the supervisor process). This distinguishes between:
    // - An agent running inside a supervised session (should use completion signals)
    // - A user manually running commands while a supervisor is active (should complete directly)
    if lock::is_current_process_supervised(&lock_dir)? {
        let signal = completions::CompletionSignal {
            task_id: task_id.to_string(),
            status,
            notes: notes.to_vec(),
        };
        let path = completions::write_completion_signal(&resolved.repo_root, &signal)?;
        log::info!(
            "Running under supervision - wrote completion signal at {}",
            path.display()
        );
        return Ok(());
    }

    // Use "task" label to enable shared lock mode, allowing this command to work
    // concurrently with a supervising process (like `ralph run loop`).
    // This matches the behavior of `ralph task build`.
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task", force)?;
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        status,
        &now,
        notes,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    log::info!(
        "Task {} completed (status: {}) and moved to done archive.",
        task_id,
        status
    );
    Ok(())
}
