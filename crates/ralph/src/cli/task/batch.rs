//! Batch task operations for `ralph task batch` subcommand.
//!
//! Responsibilities:
//! - Handle batch status updates.
//! - Handle batch field setting.
//! - Handle batch field editing.
//! - Handle batch delete, archive, clone, split, and plan operations.
//!
//! Not handled here:
//! - Single-task operations (see `status.rs`, `edit.rs`).
//! - Task building or cloning (see `build.rs`, `clone.rs`).
//!
//! Invariants/assumptions:
//! - Supports dry-run mode for previewing changes.
//! - Supports continue-on-error mode for partial success.
//! - Task IDs can be specified explicitly or via tag filter.

use anyhow::{Result, bail};

use crate::cli::task::args::{BatchOperation, BatchSelectArgs, TaskBatchArgs};
use crate::config;
use crate::contracts::{QueueFile, TaskStatus};
use crate::queue;
use crate::timeutil;

/// Convert CLI selection args to core filters.
fn to_core_filters(select: &BatchSelectArgs) -> queue::operations::BatchTaskFilters {
    queue::operations::BatchTaskFilters {
        status_filter: select
            .status_filter
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
        priority_filter: select
            .priority_filter
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
        scope_filter: select.scope_filter.clone(),
        older_than: select.older_than.clone(),
    }
}

/// Resolve task IDs using the new filtered resolver.
fn resolve_with_filters(
    queue: &QueueFile,
    select: &BatchSelectArgs,
    now_rfc3339: &str,
) -> Result<Vec<String>> {
    let filters = to_core_filters(select);
    queue::operations::resolve_task_ids_filtered(
        queue,
        &select.task_ids,
        &select.tag_filter,
        &filters,
        now_rfc3339,
    )
}

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

            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &status_args.select, &now)?;

            if task_ids.is_empty() {
                bail!("No tasks specified. Provide task IDs or use filters.");
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

                    // Create undo snapshot before mutation
                    crate::undo::create_undo_snapshot(
                        resolved,
                        &format!("batch status {} [{} tasks]", status, task_ids.len()),
                    )?;
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
                            None,
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

                    // Create undo snapshot before mutation
                    crate::undo::create_undo_snapshot(
                        resolved,
                        &format!("batch status {} [{} tasks]", status, task_ids.len()),
                    )?;
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
            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &field_args.select, &now)?;

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

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!(
                    "batch set {}={} [{} tasks]",
                    field_args.key,
                    field_args.value,
                    task_ids.len()
                ),
            )?;

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

            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &edit_args.select, &now)?;

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

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!(
                    "batch edit {} [{} tasks]",
                    edit_args.field.as_str(),
                    task_ids.len()
                ),
            )?;

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
        BatchOperation::Delete(delete_args) => {
            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &delete_args.select, &now)?;

            if args.dry_run {
                println!(
                    "Dry run - would delete {} tasks from the queue:",
                    task_ids.len()
                );
                for task_id in &task_ids {
                    println!("  - {}", task_id);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!("batch delete [{} tasks]", task_ids.len()),
            )?;

            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_delete_tasks(
                &mut queue_file,
                &task_ids,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(&result, "Delete tasks", false);

            Ok(())
        }
        BatchOperation::Archive(archive_args) => {
            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &archive_args.select, &now)?;

            if args.dry_run {
                println!(
                    "Dry run - would archive {} terminal tasks to the configured done archive:",
                    task_ids.len()
                );
                for task_id in &task_ids {
                    // Check if task is terminal
                    if let Some(task) = queue_file.tasks.iter().find(|t| t.id == *task_id) {
                        let is_terminal =
                            matches!(task.status, TaskStatus::Done | TaskStatus::Rejected);
                        if is_terminal {
                            println!("  - {} ({})", task_id, task.status);
                        } else {
                            println!(
                                "  - {} ({} - WOULD FAIL, not terminal)",
                                task_id, task.status
                            );
                        }
                    } else {
                        println!("  - {} (not found)", task_id);
                    }
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!("batch archive [{} tasks]", task_ids.len()),
            )?;

            let mut queue_file = queue::load_queue(&resolved.queue_path)?;
            let mut done_file = queue::load_queue_or_default(&resolved.done_path)?;

            let result = queue::operations::batch_archive_tasks(
                &mut queue_file,
                &mut done_file,
                &task_ids,
                &now,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::save_queue(&resolved.done_path, &done_file)?;
            queue::operations::print_batch_results(&result, "Archive tasks", false);

            Ok(())
        }
        BatchOperation::Clone(clone_args) => {
            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &clone_args.select, &now)?;

            let status: TaskStatus = clone_args
                .status
                .map(|s| s.into())
                .unwrap_or(TaskStatus::Draft);

            if args.dry_run {
                println!(
                    "Dry run - would clone {} tasks with status '{}':",
                    task_ids.len(),
                    status
                );
                for task_id in &task_ids {
                    let prefix_info = clone_args
                        .title_prefix
                        .as_deref()
                        .map(|p| format!(" [prefix: '{}']", p))
                        .unwrap_or_default();
                    println!("  - {}{}", task_id, prefix_info);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!("batch clone [{} tasks]", task_ids.len()),
            )?;

            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_clone_tasks(
                &mut queue_file,
                done_ref,
                &task_ids,
                status,
                clone_args.title_prefix.as_deref(),
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(&result, "Clone tasks", false);

            Ok(())
        }
        BatchOperation::Split(split_args) => {
            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &split_args.select, &now)?;

            let status: TaskStatus = split_args
                .status
                .map(|s| s.into())
                .unwrap_or(TaskStatus::Draft);

            if args.dry_run {
                println!(
                    "Dry run - would split {} tasks into {} children each with status '{}':",
                    task_ids.len(),
                    split_args.number,
                    status
                );
                for task_id in &task_ids {
                    let dist_info = if split_args.distribute_plan {
                        " [distribute plan]"
                    } else {
                        ""
                    };
                    println!("  - {}{}", task_id, dist_info);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!("batch split [{} tasks]", task_ids.len()),
            )?;

            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_split_tasks(
                &mut queue_file,
                &task_ids,
                split_args.number,
                status,
                split_args.title_prefix.as_deref(),
                split_args.distribute_plan,
                &now,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(&result, "Split tasks", false);

            Ok(())
        }
        BatchOperation::PlanAppend(plan_args) => {
            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &plan_args.select, &now)?;

            if args.dry_run {
                println!(
                    "Dry run - would append {} plan items to {} tasks:",
                    plan_args.plan_items.len(),
                    task_ids.len()
                );
                println!("Plan items to append:");
                for item in &plan_args.plan_items {
                    println!("  - {}", item);
                }
                println!("\nTarget tasks:");
                for task_id in &task_ids {
                    println!("  - {}", task_id);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!("batch plan-append [{} tasks]", task_ids.len()),
            )?;

            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_plan_append(
                &mut queue_file,
                &task_ids,
                &plan_args.plan_items,
                &now,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(&result, "Plan append", false);

            Ok(())
        }
        BatchOperation::PlanPrepend(plan_args) => {
            // Resolve task IDs using filtered resolver
            let task_ids = resolve_with_filters(&queue_file, &plan_args.select, &now)?;

            if args.dry_run {
                println!(
                    "Dry run - would prepend {} plan items to {} tasks:",
                    plan_args.plan_items.len(),
                    task_ids.len()
                );
                println!("Plan items to prepend:");
                for item in &plan_args.plan_items {
                    println!("  - {}", item);
                }
                println!("\nTarget tasks:");
                for task_id in &task_ids {
                    println!("  - {}", task_id);
                }
                println!("\nDry run complete. No changes made.");
                return Ok(());
            }

            let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task batch", force)?;

            // Create undo snapshot before mutation
            crate::undo::create_undo_snapshot(
                resolved,
                &format!("batch plan-prepend [{} tasks]", task_ids.len()),
            )?;

            let mut queue_file = queue::load_queue(&resolved.queue_path)?;

            let result = queue::operations::batch_plan_prepend(
                &mut queue_file,
                &task_ids,
                &plan_args.plan_items,
                &now,
                args.continue_on_error,
            )?;

            queue::save_queue(&resolved.queue_path, &queue_file)?;
            queue::operations::print_batch_results(&result, "Plan prepend", false);

            Ok(())
        }
    }
}
