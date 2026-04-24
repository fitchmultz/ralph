//! Batch task operations for `ralph task batch`.
//!
//! Purpose:
//! - Batch task operations for `ralph task batch`.
//!
//! Responsibilities:
//! - Route each batch subcommand to a focused handler.
//! - Share loaded queue/done context, selector resolution, and mutation scaffolding.
//!
//! Not handled here:
//! - Queue mutation logic itself (see `crate::queue::operations`).
//! - Single-task task commands.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Supports dry-run previews and continue-on-error consistently across operations.

#[path = "batch/context.rs"]
mod context;
#[path = "batch/dry_run.rs"]
mod dry_run;
#[path = "batch/mutate.rs"]
mod mutate;
#[path = "batch/select.rs"]
mod select;
#[path = "batch/status.rs"]
mod status;

use anyhow::Result;

use crate::cli::task::args::{BatchOperation, TaskBatchArgs};
use crate::config;
use crate::contracts::TaskStatus;

pub fn handle(args: &TaskBatchArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let ctx = context::BatchContext::load(resolved)?;

    match &args.operation {
        BatchOperation::Status(status_args) => {
            let task_ids = select::require_task_ids(select::resolve_with_filters(
                &ctx.queue_file,
                &status_args.select,
                &ctx.now,
            )?)?;
            status::handle_status(
                &ctx,
                args,
                force,
                task_ids,
                status_args.status.into(),
                status_args.note.as_deref(),
            )
        }
        BatchOperation::Field(field_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &field_args.select, &ctx.now)?;
            mutate::handle_set_field(
                &ctx,
                args,
                force,
                task_ids,
                &field_args.key,
                &field_args.value,
            )
        }
        BatchOperation::Edit(edit_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &edit_args.select, &ctx.now)?;
            mutate::handle_edit(
                &ctx,
                args,
                force,
                task_ids,
                edit_args.field,
                &edit_args.value,
            )
        }
        BatchOperation::Delete(delete_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &delete_args.select, &ctx.now)?;
            mutate::handle_delete(&ctx, args, force, task_ids)
        }
        BatchOperation::Archive(archive_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &archive_args.select, &ctx.now)?;
            mutate::handle_archive(&ctx, args, force, task_ids)
        }
        BatchOperation::Clone(clone_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &clone_args.select, &ctx.now)?;
            let status: TaskStatus = clone_args
                .status
                .map(Into::into)
                .unwrap_or(TaskStatus::Draft);
            mutate::handle_clone(
                &ctx,
                args,
                force,
                task_ids,
                status,
                clone_args.title_prefix.as_deref(),
            )
        }
        BatchOperation::Split(split_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &split_args.select, &ctx.now)?;
            let status: TaskStatus = split_args
                .status
                .map(Into::into)
                .unwrap_or(TaskStatus::Draft);
            mutate::handle_split(
                &ctx,
                args,
                force,
                task_ids,
                split_args.number,
                status,
                split_args.title_prefix.as_deref(),
                split_args.distribute_plan,
            )
        }
        BatchOperation::PlanAppend(plan_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &plan_args.select, &ctx.now)?;
            mutate::handle_plan_append(&ctx, args, force, task_ids, &plan_args.plan_items)
        }
        BatchOperation::PlanPrepend(plan_args) => {
            let task_ids =
                select::resolve_with_filters(&ctx.queue_file, &plan_args.select, &ctx.now)?;
            mutate::handle_plan_prepend(&ctx, args, force, task_ids, &plan_args.plan_items)
        }
    }
}
