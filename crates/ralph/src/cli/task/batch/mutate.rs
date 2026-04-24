//! Mutation handlers for non-status batch task operations.
//!
//! Purpose:
//! - Mutation handlers for non-status batch task operations.
//!
//! Responsibilities:
//! - Execute each batch mutation against shared queue operation helpers.
//! - Keep per-operation persistence and dry-run output localized.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::cli::task::args::{TaskBatchArgs, TaskEditFieldArg};
use crate::cli::task::batch::{context::BatchContext, dry_run};
use crate::contracts::TaskStatus;
use crate::queue;
use anyhow::Result;

pub(super) fn handle_set_field(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
    key: &str,
    value: &str,
) -> Result<()> {
    if args.dry_run {
        dry_run::field(key, value, &task_ids);
        return Ok(());
    }

    let _queue_lock = ctx.begin_mutation(
        force,
        &format!("batch set {}={} [{} tasks]", key, value, task_ids.len()),
    )?;
    let mut queue_file = ctx.reload_queue()?;
    let result = queue::operations::batch_set_field(
        &mut queue_file,
        &task_ids,
        key,
        value,
        &ctx.now,
        args.continue_on_error,
    )?;
    ctx.save_queue(&queue_file)?;
    queue::operations::print_batch_results(
        &result,
        &format!("Field set '{}' = '{}'", key, value),
        false,
    );
    Ok(())
}

pub(super) fn handle_edit(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
    field: TaskEditFieldArg,
    value: &str,
) -> Result<()> {
    use crate::queue::TaskEditKey;

    if args.dry_run {
        println!(
            "Dry run - would edit field '{}' to '{}' on {} tasks:",
            field.as_str(),
            value,
            task_ids.len()
        );
        for task_id in &task_ids {
            let preview = queue::preview_task_edit(
                &ctx.queue_file,
                ctx.done_ref(),
                task_id,
                TaskEditKey::from(field),
                value,
                &ctx.now,
                &ctx.resolved.id_prefix,
                ctx.resolved.id_width,
                ctx.max_depth,
            )?;
            println!("  {}:", preview.task_id);
            println!("    Old: {}", preview.old_value);
            println!("    New: {}", preview.new_value);
        }
        println!("\nDry run complete. No changes made.");
        return Ok(());
    }

    let _queue_lock = ctx.begin_mutation(
        force,
        &format!("batch edit {} [{} tasks]", field.as_str(), task_ids.len()),
    )?;
    let mut queue_file = ctx.reload_queue()?;
    let result = queue::operations::batch_apply_edit(
        &mut queue_file,
        ctx.done_ref(),
        &task_ids,
        TaskEditKey::from(field),
        value,
        &ctx.now,
        &ctx.resolved.id_prefix,
        ctx.resolved.id_width,
        ctx.max_depth,
        args.continue_on_error,
    )?;
    ctx.save_queue(&queue_file)?;
    queue::operations::print_batch_results(
        &result,
        &format!("Edit field '{}'", field.as_str()),
        false,
    );
    Ok(())
}

pub(super) fn handle_delete(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
) -> Result<()> {
    if args.dry_run {
        dry_run::simple(
            &format!(
                "Dry run - would delete {} tasks from the queue:",
                task_ids.len()
            ),
            &task_ids,
        );
        return Ok(());
    }
    let _queue_lock =
        ctx.begin_mutation(force, &format!("batch delete [{} tasks]", task_ids.len()))?;
    let mut queue_file = ctx.reload_queue()?;
    let result =
        queue::operations::batch_delete_tasks(&mut queue_file, &task_ids, args.continue_on_error)?;
    ctx.save_queue(&queue_file)?;
    queue::operations::print_batch_results(&result, "Delete tasks", false);
    Ok(())
}

pub(super) fn handle_archive(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
) -> Result<()> {
    if args.dry_run {
        println!(
            "Dry run - would archive {} terminal tasks to the configured done archive:",
            task_ids.len()
        );
        for task_id in &task_ids {
            if let Some(task) = ctx.queue_file.tasks.iter().find(|t| t.id == *task_id) {
                let is_terminal = matches!(task.status, TaskStatus::Done | TaskStatus::Rejected);
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

    let _queue_lock =
        ctx.begin_mutation(force, &format!("batch archive [{} tasks]", task_ids.len()))?;
    let mut queue_file = ctx.reload_queue()?;
    let mut done_file = ctx.reload_done()?;
    let result = queue::operations::batch_archive_tasks(
        &mut queue_file,
        &mut done_file,
        &task_ids,
        &ctx.now,
        args.continue_on_error,
    )?;
    ctx.save_queue(&queue_file)?;
    ctx.save_done(&done_file)?;
    queue::operations::print_batch_results(&result, "Archive tasks", false);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_clone(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
    status: TaskStatus,
    title_prefix: Option<&str>,
) -> Result<()> {
    if args.dry_run {
        dry_run::clone_tasks(status, title_prefix, &task_ids);
        return Ok(());
    }

    let _queue_lock =
        ctx.begin_mutation(force, &format!("batch clone [{} tasks]", task_ids.len()))?;
    let mut queue_file = ctx.reload_queue()?;
    let result = queue::operations::batch_clone_tasks(
        &mut queue_file,
        ctx.done_ref(),
        &task_ids,
        status,
        title_prefix,
        &ctx.now,
        &ctx.resolved.id_prefix,
        ctx.resolved.id_width,
        ctx.max_depth,
        args.continue_on_error,
    )?;
    ctx.save_queue(&queue_file)?;
    queue::operations::print_batch_results(&result, "Clone tasks", false);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_split(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
    number: usize,
    status: TaskStatus,
    title_prefix: Option<&str>,
    distribute_plan: bool,
) -> Result<()> {
    if args.dry_run {
        dry_run::split_tasks(number, status, distribute_plan, &task_ids);
        return Ok(());
    }

    let _queue_lock =
        ctx.begin_mutation(force, &format!("batch split [{} tasks]", task_ids.len()))?;
    let mut queue_file = ctx.reload_queue()?;
    let result = queue::operations::batch_split_tasks(
        &mut queue_file,
        &task_ids,
        number,
        status,
        title_prefix,
        distribute_plan,
        &ctx.now,
        &ctx.resolved.id_prefix,
        ctx.resolved.id_width,
        ctx.max_depth,
        args.continue_on_error,
    )?;
    ctx.save_queue(&queue_file)?;
    queue::operations::print_batch_results(&result, "Split tasks", false);
    Ok(())
}

pub(super) fn handle_plan_append(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
    plan_items: &[String],
) -> Result<()> {
    if args.dry_run {
        dry_run::plan_items("append", plan_items, &task_ids);
        return Ok(());
    }

    let _queue_lock = ctx.begin_mutation(
        force,
        &format!("batch plan-append [{} tasks]", task_ids.len()),
    )?;
    let mut queue_file = ctx.reload_queue()?;
    let result = queue::operations::batch_plan_append(
        &mut queue_file,
        &task_ids,
        plan_items,
        &ctx.now,
        args.continue_on_error,
    )?;
    ctx.save_queue(&queue_file)?;
    queue::operations::print_batch_results(&result, "Plan append", false);
    Ok(())
}

pub(super) fn handle_plan_prepend(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
    plan_items: &[String],
) -> Result<()> {
    if args.dry_run {
        dry_run::plan_items("prepend", plan_items, &task_ids);
        return Ok(());
    }

    let _queue_lock = ctx.begin_mutation(
        force,
        &format!("batch plan-prepend [{} tasks]", task_ids.len()),
    )?;
    let mut queue_file = ctx.reload_queue()?;
    let result = queue::operations::batch_plan_prepend(
        &mut queue_file,
        &task_ids,
        plan_items,
        &ctx.now,
        args.continue_on_error,
    )?;
    ctx.save_queue(&queue_file)?;
    queue::operations::print_batch_results(&result, "Plan prepend", false);
    Ok(())
}
