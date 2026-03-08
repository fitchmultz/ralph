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
//! - Terminal statuses (done, rejected) archive tasks to the done archive file.
//! - Non-terminal statuses update in-place in the active queue file.
//! - Tasks are always completed directly via queue::complete_task.

use anyhow::{Result, bail};

use crate::cli::task::args::{TaskDoneArgs, TaskReadyArgs, TaskRejectArgs, TaskStatusArgs};
use crate::config;
use crate::constants::custom_fields::{MODEL_USED, RUNNER_USED};
use crate::contracts::TaskStatus;
use crate::queue;
use crate::queue::operations::{
    TaskFieldEdit, TaskMutationRequest, TaskMutationSpec, apply_task_mutation_request,
};
use crate::timeutil;
use crate::webhook;
use std::collections::HashMap;

/// Handle the `ready` command (promote draft to todo).
pub fn handle_ready(args: &TaskReadyArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task ready", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(resolved, &format!("task ready {}", args.task_id))?;

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
            // because complete_task involves moving tasks to the done archive
            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task status", force)?;

            // Create undo snapshot before mutation
            // Note: for batch operations, we create one snapshot for the whole batch
            let task_ids_preview = args.task_ids.join(", ");
            crate::undo::create_undo_snapshot(
                resolved,
                &format!(
                    "task status {} -> {} [{} tasks]",
                    task_ids_preview,
                    status,
                    args.task_ids.len()
                ),
            )?;

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
                    None,
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

            // Create undo snapshot before mutation
            let task_ids_preview = args.task_ids.join(", ");
            crate::undo::create_undo_snapshot(
                resolved,
                &format!(
                    "task status {} -> {} [{} tasks]",
                    task_ids_preview,
                    status,
                    args.task_ids.len()
                ),
            )?;

            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let now = timeutil::now_utc_rfc3339()?;

            // Resolve task IDs from explicit list or tag filter
            let task_ids =
                queue::operations::resolve_task_ids(&queue_file, &args.task_ids, &args.tag_filter)?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            if let Some(note) = args.note.as_deref()
                && !note.trim().is_empty()
            {
                let result = queue::operations::batch_set_status(
                    &mut queue_file,
                    &task_ids,
                    status,
                    &now,
                    Some(note),
                    false,
                )?;
                queue::save_queue(&resolved.queue_path, &queue_file)?;
                queue::operations::print_batch_results(
                    &result,
                    &format!("Status update to {}", status),
                    false,
                );
                return Ok(());
            }

            let request = TaskMutationRequest {
                version: 1,
                atomic: true,
                tasks: task_ids
                    .iter()
                    .map(|task_id| TaskMutationSpec {
                        task_id: task_id.clone(),
                        expected_updated_at: None,
                        edits: vec![TaskFieldEdit {
                            field: "status".to_string(),
                            value: status.to_string(),
                        }],
                    })
                    .collect(),
            };

            let result = apply_task_mutation_request(
                &mut queue_file,
                None,
                &request,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                resolved.config.queue.max_dependency_depth.unwrap_or(10),
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            println!("Updated {} task(s) to {}.", result.tasks.len(), status);

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
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(resolved, &format!("task {} {}", status, task_id))?;

    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Build custom fields patch from environment variables
    let custom_fields_patch = build_custom_fields_patch_from_env();

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
    )?;
    log::info!(
        "Task {} completed (status: {}) and moved to done archive.",
        task_id,
        status
    );
    Ok(())
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
