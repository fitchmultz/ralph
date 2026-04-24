//! Status-specific batch handlers.
//!
//! Purpose:
//! - Status-specific batch handlers.
//!
//! Responsibilities:
//! - Handle terminal status updates that archive tasks individually.
//! - Handle non-terminal status updates through shared queue batch helpers.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::cli::task::args::TaskBatchArgs;
use crate::cli::task::batch::{context::BatchContext, dry_run};
use crate::contracts::TaskStatus;
use crate::queue;
use anyhow::{Result, bail};

pub(super) fn handle_status(
    ctx: &BatchContext<'_>,
    args: &TaskBatchArgs,
    force: bool,
    task_ids: Vec<String>,
    status: TaskStatus,
    note: Option<&str>,
) -> Result<()> {
    match status {
        TaskStatus::Done | TaskStatus::Rejected => {
            if args.dry_run {
                dry_run::terminal_status(status, &task_ids);
                return Ok(());
            }

            let _queue_lock = ctx.begin_mutation(
                force,
                &format!("batch status {} [{} tasks]", status, task_ids.len()),
            )?;
            let mut results = Vec::new();
            let mut succeeded = 0;
            let mut failed = 0;

            for task_id in &task_ids {
                match queue::complete_task(
                    &ctx.resolved.queue_path,
                    &ctx.resolved.done_path,
                    task_id,
                    status,
                    &ctx.now,
                    note.map(|n| vec![n.to_string()]).as_deref().unwrap_or(&[]),
                    &ctx.resolved.id_prefix,
                    ctx.resolved.id_width,
                    ctx.max_depth,
                    None,
                ) {
                    Ok(()) => {
                        results.push((task_id.clone(), true, None));
                        succeeded += 1;
                    }
                    Err(err) => {
                        let err_msg = err.to_string();
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
                dry_run::status(status, &task_ids);
                return Ok(());
            }

            let _queue_lock = ctx.begin_mutation(
                force,
                &format!("batch status {} [{} tasks]", status, task_ids.len()),
            )?;
            let mut queue_file = ctx.reload_queue()?;
            let result = queue::operations::batch_set_status(
                &mut queue_file,
                &task_ids,
                status,
                &ctx.now,
                note,
                args.continue_on_error,
            )?;
            ctx.save_queue(&queue_file)?;
            queue::operations::print_batch_results(
                &result,
                &format!("Status update to {}", status),
                false,
            );
            Ok(())
        }
    }
}
