//! Batch task operations for `ralph task batch` subcommand.
//!
//! Responsibilities:
//! - Handle batch status updates.
//! - Handle batch field setting.
//! - Handle batch field editing.
//!
//! Not handled here:
//! - Single-task operations (see `status.rs`, `edit.rs`).
//! - Task building or cloning (see `build.rs`, `clone.rs`).
//!
//! Invariants/assumptions:
//! - Supports dry-run mode for previewing changes.
//! - Supports continue-on-error mode for partial success.
//! - Task IDs can be specified explicitly or via tag filter.

use anyhow::{bail, Result};

use crate::cli::task::args::{BatchOperation, TaskBatchArgs};
use crate::config;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::timeutil;

/// Handle batch task operations.
pub fn handle(args: &TaskBatchArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    match &args.operation {
        BatchOperation::Status(status_args) => {
            let status: TaskStatus = status_args.status.into();

            // Resolve task IDs from explicit list or tag filter
            let task_ids = queue::operations::resolve_task_ids(
                &queue_file,
                &status_args.task_ids,
                &status_args.tag_filter,
            )?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            // For terminal statuses, use complete_task for each task
            match status {
                TaskStatus::Done | TaskStatus::Rejected => {
                    if args.dry_run {
                        println!(
                            "Dry run - would mark {} tasks as {} and archive them:",
                            task_ids.len(),
                            status
                        );
                        for task_id in &task_ids {
                            println!("  - {}", task_id);
                        }
                        println!("\nDry run complete. No changes made.");
                        return Ok(());
                    }

                    let _queue_lock =
                        queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
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
                            status_args
                                .note
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
                                let err_msg = e.to_string();
                                results.push((task_id.clone(), false, Some(err_msg.clone())));
                                failed += 1;

                                if !args.continue_on_error {
                                    bail!(
                                        "Batch operation failed at task {}: {}. Use --continue-on-error to process remaining tasks.",
                                        task_id,
                                        err_msg
                                    );
                                }
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
                    if args.dry_run {
                        println!(
                            "Dry run - would update {} tasks to status '{}':",
                            task_ids.len(),
                            status
                        );
                        for task_id in &task_ids {
                            println!("  - {}", task_id);
                        }
                        println!("\nDry run complete. No changes made.");
                        return Ok(());
                    }

                    let _queue_lock =
                        queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
                    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

                    let result = queue::operations::batch_set_status(
                        &mut queue_file,
                        &task_ids,
                        status,
                        &now,
                        status_args.note.as_deref(),
                        args.continue_on_error,
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
        BatchOperation::Field(field_args) => {
            // Resolve task IDs from explicit list or tag filter
            let task_ids = queue::operations::resolve_task_ids(
                &queue_file,
                &field_args.task_ids,
                &field_args.tag_filter,
            )?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            if args.dry_run {
                println!(
                    "Dry run - would set field '{}' = '{}' on {} tasks:",
                    field_args.key,
                    field_args.value,
                    task_ids.len()
                );
                for task_id in &task_ids {
                    println!("  - {}", task_id);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_set_field(
                &mut queue_file,
                &task_ids,
                &field_args.key,
                &field_args.value,
                &now,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(
                &result,
                &format!("Field set '{}' = '{}'", field_args.key, field_args.value),
                false,
            );

            Ok(())
        }
        BatchOperation::Edit(edit_args) => {
            use crate::queue::TaskEditKey;

            // Resolve task IDs from explicit list or tag filter
            let task_ids = queue::operations::resolve_task_ids(
                &queue_file,
                &edit_args.task_ids,
                &edit_args.tag_filter,
            )?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use --tag-filter.");
            }

            if args.dry_run {
                println!(
                    "Dry run - would edit field '{}' to '{}' on {} tasks:",
                    edit_args.field.as_str(),
                    edit_args.value,
                    task_ids.len()
                );
                for task_id in &task_ids {
                    let preview = queue::preview_task_edit(
                        &queue_file,
                        done_ref,
                        task_id,
                        TaskEditKey::from(edit_args.field),
                        &edit_args.value,
                        &now,
                        &resolved.id_prefix,
                        resolved.id_width,
                        max_depth,
                    )?;
                    println!("  {}:", preview.task_id);
                    println!("    Old: {}", preview.old_value);
                    println!("    New: {}", preview.new_value);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;
            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_apply_edit(
                &mut queue_file,
                done_ref,
                &task_ids,
                TaskEditKey::from(edit_args.field),
                &edit_args.value,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(
                &result,
                &format!("Edit field '{}'", edit_args.field.as_str()),
                false,
            );

            Ok(())
        }
    }
}
