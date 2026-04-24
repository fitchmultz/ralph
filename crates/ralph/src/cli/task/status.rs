//! Task status command handlers for `ralph task` subcommands.
//!
//! Purpose:
//! - Task status command handlers for `ralph task` subcommands.
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
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Terminal statuses (done, rejected) archive tasks to the done archive file.
//! - Non-terminal statuses update in-place in the active queue file.
//! - Tasks are always completed directly via queue::complete_task.

use anyhow::{Result, bail};

use crate::cli::task::args::{TaskDoneArgs, TaskReadyArgs, TaskRejectArgs, TaskStatusArgs};
use crate::config;
use crate::constants::custom_fields::{MODEL_USED, RUNNER_USED};
use crate::contracts::TaskStatus;
use crate::queue;
use crate::queue::operations::{batch_set_status, print_batch_results, resolve_task_ids};
use crate::timeutil;
use crate::webhook;
use std::collections::HashMap;

/// Handle the `ready` command (promote draft to todo).
pub fn handle_ready(args: &TaskReadyArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let mut task_info = None;
    let now = queue::with_locked_queue_mutation(
        resolved,
        "task ready",
        format!("task ready {}", args.task_id),
        force,
        || {
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;

            task_info = queue_file
                .tasks
                .iter()
                .find(|t| t.id == args.task_id)
                .map(|t| (t.id.clone(), t.title.clone()));

            queue::promote_draft_to_todo(
                &mut queue_file,
                &args.task_id,
                &now,
                args.note.as_deref(),
            )?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
            Ok(now)
        },
    )?;

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
    if matches!(status, TaskStatus::Done | TaskStatus::Rejected) {
        return handle_terminal_status(args, force, resolved, status);
    }

    handle_non_terminal_status(args, force, resolved, status)
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

    complete_task_directly(resolved, &args.task_id, TaskStatus::Done, &args.note, force)?;

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
    complete_task_directly(
        resolved,
        &args.task_id,
        TaskStatus::Rejected,
        &args.note,
        force,
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

/// Complete a task directly in queue/done.
fn complete_task_directly(
    resolved: &config::Resolved,
    task_id: &str,
    status: TaskStatus,
    notes: &[String],
    force: bool,
) -> Result<()> {
    // Use "task" label to enable shared lock mode, allowing this command to work
    // concurrently with a supervising process (like `ralph run loop`).
    // This matches the behavior of `ralph task build`.
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let custom_fields_patch = build_custom_fields_patch_from_env();

    queue::with_locked_queue_mutation(
        resolved,
        "task",
        format!("task {} {}", status, task_id),
        force,
        || {
            let now = timeutil::now_utc_rfc3339()?;
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
                custom_fields_patch.as_ref(),
            )
        },
    )?;
    log::info!(
        "Task {} completed (status: {}) and moved to done archive.",
        task_id,
        status
    );
    Ok(())
}

fn handle_non_terminal_status(
    args: &TaskStatusArgs,
    force: bool,
    resolved: &config::Resolved,
    status: TaskStatus,
) -> Result<()> {
    queue::with_locked_queue_mutation(
        resolved,
        "task status",
        status_operation(args, status),
        force,
        || {
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            let task_ids = resolved_status_task_ids(&queue_file, args)?;
            let result = batch_set_status(
                &mut queue_file,
                &task_ids,
                status,
                &now,
                args.note.as_deref().filter(|note| !note.trim().is_empty()),
                false,
            )?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
            print_batch_results(&result, &format!("Status update to {}", status), false);
            Ok(())
        },
    )
}

fn handle_terminal_status(
    args: &TaskStatusArgs,
    force: bool,
    resolved: &config::Resolved,
    status: TaskStatus,
) -> Result<()> {
    queue::with_locked_queue_mutation(
        resolved,
        "task status",
        status_operation(args, status),
        force,
        || {
            let queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;
            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
            let task_ids = resolved_status_task_ids(&queue_file, args)?;
            let notes = args.note.as_deref().map(|note| vec![note.to_string()]);
            let notes = notes.as_deref().unwrap_or(&[]);

            let mut succeeded = Vec::new();
            let mut failures = Vec::new();
            for task_id in &task_ids {
                match queue::complete_task(
                    &resolved.queue_path,
                    &resolved.done_path,
                    task_id,
                    status,
                    &now,
                    notes,
                    &resolved.id_prefix,
                    resolved.id_width,
                    max_depth,
                    None,
                ) {
                    Ok(()) => succeeded.push(task_id.clone()),
                    Err(err) => failures.push((task_id.clone(), err.to_string())),
                }
            }

            print_terminal_status_results(status, task_ids.len(), &succeeded, &failures);
            if failures.is_empty() {
                Ok(())
            } else {
                bail!(
                    "task status completed with {} failed task(s).",
                    failures.len()
                )
            }
        },
    )
}

fn resolved_status_task_ids(
    queue_file: &crate::contracts::QueueFile,
    args: &TaskStatusArgs,
) -> Result<Vec<String>> {
    let task_ids = resolve_task_ids(queue_file, &args.task_ids, &args.tag_filter)?;
    if task_ids.is_empty() {
        bail!("No tasks specified. Provide task IDs or use --tag-filter.");
    }
    Ok(task_ids)
}

fn status_operation(args: &TaskStatusArgs, status: TaskStatus) -> String {
    format!(
        "task status {} -> {} [{} tasks]",
        args.task_ids.join(", "),
        status,
        args.task_ids.len()
    )
}

fn print_terminal_status_results(
    status: TaskStatus,
    total: usize,
    succeeded: &[String],
    failures: &[(String, String)],
) {
    if failures.is_empty() {
        println!(
            "Successfully marked {} tasks as {}:",
            succeeded.len(),
            status
        );
        for task_id in succeeded {
            println!("  ✓ {}", task_id);
        }
        return;
    }

    println!("Completed with errors:");
    for task_id in succeeded {
        println!("  ✓ {}: {} and archived", task_id, status);
    }
    for (task_id, error) in failures {
        println!("  ✗ {}: failed - {}", task_id, error);
    }
    println!(
        "Completed: {}/{} tasks {} successfully.",
        succeeded.len(),
        total,
        status
    );
}

/// Read an environment variable and return Some(trimmed value) if non-empty.
fn read_env_trimmed(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Build custom fields patch from environment variables for observational analytics.
fn build_custom_fields_patch_from_env() -> Option<HashMap<String, String>> {
    // These environment variables are set by the runner or external tools
    // and provide observational analytics about what was actually used.
    let runner_key = "RALPH_RUNNER_USED";
    let model_key = "RALPH_MODEL_USED";

    let mut patch = HashMap::new();

    if let Some(runner) = read_env_trimmed(runner_key) {
        patch.insert(RUNNER_USED.to_string(), runner.to_ascii_lowercase());
    }
    if let Some(model) = read_env_trimmed(model_key) {
        patch.insert(MODEL_USED.to_string(), model.to_string());
    }

    if patch.is_empty() { None } else { Some(patch) }
}
