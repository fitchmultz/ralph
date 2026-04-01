//! Queue continuation document builders for machine queue commands.
//!
//! Purpose:
//! - Build the machine queue validation, repair, and undo payloads.
//!
//! Responsibilities:
//! - Encode queue continuation states and next-step guidance.
//! - Keep continuation JSON deterministic and versioned.
//! - Own the helper logic shared by queue validation/repair/undo documents.
//!
//! Scope:
//! - Machine queue continuation document construction only.
//! - Does not route CLI commands or print output.
//!
//! Usage:
//! - Called from `queue.rs`, which handles `ralph machine queue ...` dispatch.
//! - Re-exported by `cli::machine::mod` for crate-internal CLI consumers.
//!
//! Invariants/assumptions:
//! - Queue reads remain read-only.
//! - Repair/undo writes hold the queue lock and create undo checkpoints first.

use anyhow::Result;

use crate::cli::machine::args::{MachineQueueRepairArgs, MachineQueueUndoArgs};
use crate::cli::machine::common::{done_queue_ref, queue_max_dependency_depth};
use crate::contracts::{
    BlockingState, BlockingStatus, MACHINE_QUEUE_REPAIR_VERSION, MACHINE_QUEUE_UNDO_VERSION,
    MACHINE_QUEUE_VALIDATE_VERSION, MachineContinuationAction, MachineContinuationSummary,
    MachineQueueRepairDocument, MachineQueueUndoDocument, MachineQueueValidateDocument,
    MachineValidationWarning,
};
use crate::queue;
use crate::queue::operations::{RunnableSelectionOptions, queue_runnability_report};

pub(crate) fn build_validate_document(
    resolved: &crate::config::Resolved,
) -> MachineQueueValidateDocument {
    let queue_file = match queue::load_queue(&resolved.queue_path) {
        Ok(queue_file) => queue_file,
        Err(err) => {
            let blocking = queue_validation_failed_state(err.to_string());
            return MachineQueueValidateDocument {
                version: MACHINE_QUEUE_VALIDATE_VERSION,
                valid: false,
                blocking: Some(blocking.clone()),
                warnings: Vec::new(),
                continuation: MachineContinuationSummary {
                    headline: "Queue continuation is stalled.".to_string(),
                    detail: "Validate failed before Ralph could confirm a safe continuation state."
                        .to_string(),
                    blocking: Some(blocking),
                    next_steps: vec![
                        step(
                            "Preview safe normalization",
                            "ralph queue repair --dry-run",
                            "Inspect recoverable fixes without writing queue files.",
                        ),
                        step(
                            "Apply normalization",
                            "ralph queue repair",
                            "Write recoverable fixes and create an undo checkpoint first.",
                        ),
                        step(
                            "Preview a restore",
                            "ralph undo --dry-run",
                            "Inspect the latest continuation checkpoint before writing more changes.",
                        ),
                    ],
                },
            };
        }
    };

    let done_file = match queue::load_queue_or_default(&resolved.done_path) {
        Ok(done_file) => Some(done_file),
        Err(err) => {
            let blocking = queue_validation_failed_state(err.to_string());
            return MachineQueueValidateDocument {
                version: MACHINE_QUEUE_VALIDATE_VERSION,
                valid: false,
                blocking: Some(blocking.clone()),
                warnings: Vec::new(),
                continuation: MachineContinuationSummary {
                    headline: "Queue continuation is stalled.".to_string(),
                    detail: "The done archive could not be loaded, so Ralph cannot confirm a safe continuation state.".to_string(),
                    blocking: Some(blocking),
                    next_steps: vec![
                        step(
                            "Preview safe normalization",
                            "ralph queue repair --dry-run",
                            "Inspect whether Ralph can normalize the queue and done archive safely.",
                        ),
                        step(
                            "Apply normalization",
                            "ralph queue repair",
                            "Write recoverable fixes and create an undo checkpoint first.",
                        ),
                        step(
                            "Preview a restore",
                            "ralph undo --dry-run",
                            "Inspect the latest continuation checkpoint before writing more changes.",
                        ),
                    ],
                },
            };
        }
    };

    let done_ref = done_file
        .as_ref()
        .and_then(|done| done_queue_ref(done, &resolved.done_path));

    match queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        queue_max_dependency_depth(resolved),
    ) {
        Ok(warnings) => {
            let warning_values = warnings
                .into_iter()
                .map(|warning| MachineValidationWarning {
                    task_id: warning.task_id,
                    message: warning.message,
                })
                .collect::<Vec<_>>();
            let runnability = queue_runnability_report(
                &queue_file,
                done_ref,
                RunnableSelectionOptions::new(false, false),
            )
            .ok();
            let blocking = runnability
                .as_ref()
                .and_then(|report| report.summary.blocking.clone());
            let continuation = continuation_for_valid_queue(blocking.clone(), &warning_values);

            MachineQueueValidateDocument {
                version: MACHINE_QUEUE_VALIDATE_VERSION,
                valid: true,
                blocking,
                warnings: warning_values,
                continuation,
            }
        }
        Err(err) => {
            let blocking = queue_validation_failed_state(err.to_string());
            MachineQueueValidateDocument {
                version: MACHINE_QUEUE_VALIDATE_VERSION,
                valid: false,
                blocking: Some(blocking.clone()),
                warnings: Vec::new(),
                continuation: MachineContinuationSummary {
                    headline: "Queue continuation is stalled.".to_string(),
                    detail: "The queue is not in a valid state for normal continuation."
                        .to_string(),
                    blocking: Some(blocking),
                    next_steps: vec![
                        step(
                            "Preview safe normalization",
                            "ralph queue repair --dry-run",
                            "See which recoverable issues Ralph can normalize.",
                        ),
                        step(
                            "Apply safe normalization",
                            "ralph queue repair",
                            "Repair recoverable issues and create an undo checkpoint.",
                        ),
                        step(
                            "Inspect the latest checkpoint",
                            "ralph undo --dry-run",
                            "Confirm whether restoring is safer than repairing.",
                        ),
                    ],
                },
            }
        }
    }
}

pub(crate) fn build_repair_document(
    resolved: &crate::config::Resolved,
    force: bool,
    args: &MachineQueueRepairArgs,
) -> Result<MachineQueueRepairDocument> {
    if args.dry_run {
        let _queue_lock =
            queue::acquire_queue_lock(&resolved.repo_root, "machine queue repair", force)?;
        let report = queue::repair_queue(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
            true,
        )?;
        let changed = !report.is_empty();
        let continuation = repair_preview_continuation(changed);
        return Ok(MachineQueueRepairDocument {
            version: MACHINE_QUEUE_REPAIR_VERSION,
            dry_run: true,
            changed,
            blocking: continuation.blocking.clone(),
            report: serde_json::to_value(report)?,
            continuation,
        });
    }

    let _queue_lock =
        queue::acquire_queue_lock(&resolved.repo_root, "machine queue repair", force)?;
    let preview = queue::repair_queue(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        true,
    )?;
    if preview.is_empty() {
        let continuation = MachineContinuationSummary {
            headline: "No queue repair changes were needed.".to_string(),
            detail: "Ralph confirmed the queue already matches its continuation invariants."
                .to_string(),
            blocking: None,
            next_steps: vec![step(
                "Continue work",
                "ralph run resume",
                "No recovery write is required before continuing.",
            )],
        };
        return Ok(MachineQueueRepairDocument {
            version: MACHINE_QUEUE_REPAIR_VERSION,
            dry_run: false,
            changed: false,
            blocking: continuation.blocking.clone(),
            report: serde_json::to_value(preview)?,
            continuation,
        });
    }

    crate::undo::create_undo_snapshot(resolved, "queue repair continuation")?;
    let report = queue::repair_queue(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
        false,
    )?;
    let continuation = MachineContinuationSummary {
        headline: "Queue continuation has been normalized.".to_string(),
        detail: "Recoverable queue issues were repaired and an undo checkpoint was created before the write.".to_string(),
        blocking: None,
        next_steps: vec![
            step(
                "Validate the normalized queue",
                "ralph queue validate",
                "Confirm the post-repair continuation state.",
            ),
            step(
                "Preview a rollback",
                "ralph undo --dry-run",
                "Inspect the restore path for this repair if you want to undo it.",
            ),
        ],
    };
    Ok(MachineQueueRepairDocument {
        version: MACHINE_QUEUE_REPAIR_VERSION,
        dry_run: false,
        changed: true,
        blocking: continuation.blocking.clone(),
        report: serde_json::to_value(report)?,
        continuation,
    })
}

pub(crate) fn build_undo_document(
    resolved: &crate::config::Resolved,
    force: bool,
    args: &MachineQueueUndoArgs,
) -> Result<MachineQueueUndoDocument> {
    if args.list {
        let list = crate::undo::list_undo_snapshots(&resolved.repo_root)?;
        let next_steps = if list.snapshots.is_empty() {
            vec![step(
                "Create a future checkpoint",
                "ralph task mutate --dry-run",
                "Most queue-changing workflows create an undo checkpoint automatically before writing.",
            )]
        } else {
            vec![
                step(
                    "Preview a restore",
                    "ralph undo --dry-run",
                    "Inspect the most recent checkpoint before restoring.",
                ),
                step(
                    "Restore a specific checkpoint",
                    "ralph undo --id <SNAPSHOT_ID>",
                    "Return to a selected queue state.",
                ),
            ]
        };
        let continuation = MachineContinuationSummary {
            headline: if next_steps.len() == 1 {
                "No continuation checkpoints are available.".to_string()
            } else {
                "Continuation checkpoints are available.".to_string()
            },
            detail: "Use a checkpoint ID or restore the most recent snapshot to continue from an earlier queue state.".to_string(),
            blocking: None,
            next_steps,
        };
        return Ok(MachineQueueUndoDocument {
            version: MACHINE_QUEUE_UNDO_VERSION,
            dry_run: true,
            restored: false,
            blocking: continuation.blocking.clone(),
            result: Some(serde_json::to_value(list.snapshots)?),
            continuation,
        });
    }

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "machine queue undo", force)?;
    let result = crate::undo::restore_from_snapshot(resolved, args.id.as_deref(), args.dry_run)?;

    let continuation = MachineContinuationSummary {
        headline: if args.dry_run {
            "Restore preview is ready.".to_string()
        } else {
            "Continuation has been restored.".to_string()
        },
        detail: if args.dry_run {
            "Ralph showed the checkpoint that would be restored without changing queue files."
                .to_string()
        } else {
            "Ralph restored the selected checkpoint. Continue from the restored queue state."
                .to_string()
        },
        blocking: None,
        next_steps: vec![
            step(
                "Validate restored state",
                "ralph queue validate",
                "Confirm the restored queue is ready.",
            ),
            step(
                "Resume normal work",
                "ralph run resume",
                "Continue from the restored queue state.",
            ),
        ],
    };
    Ok(MachineQueueUndoDocument {
        version: MACHINE_QUEUE_UNDO_VERSION,
        dry_run: args.dry_run,
        restored: !args.dry_run,
        blocking: continuation.blocking.clone(),
        result: Some(serde_json::to_value(result)?),
        continuation,
    })
}

fn queue_validation_failed_state(detail: String) -> BlockingState {
    BlockingState::operator_recovery(
        BlockingStatus::Stalled,
        "queue_validate",
        "validation_failed",
        None,
        "Ralph is stalled on queue consistency.",
        detail,
        Some("ralph queue repair --dry-run".to_string()),
    )
}

fn repair_preview_continuation(changed: bool) -> MachineContinuationSummary {
    if changed {
        return MachineContinuationSummary {
            headline: "Repair preview is ready.".to_string(),
            detail: "Ralph found recoverable queue issues and can preserve the current queue by normalizing them.".to_string(),
            blocking: Some(BlockingState::operator_recovery(
                BlockingStatus::Stalled,
                "queue_repair",
                "repair_available",
                None,
                "Ralph found recoverable queue issues.",
                "Preview completed successfully; apply the repair to continue from a normalized queue state.",
                Some("ralph queue repair".to_string()),
            )),
            next_steps: vec![
                step(
                    "Apply repair",
                    "ralph queue repair",
                    "Write recoverable fixes and create an undo checkpoint first.",
                ),
                step(
                    "Preview a rollback",
                    "ralph undo --dry-run",
                    "Inspect the restore path before applying more queue changes.",
                ),
            ],
        };
    }

    MachineContinuationSummary {
        headline: "No queue repair is needed.".to_string(),
        detail: "The queue already matches Ralph’s continuation invariants.".to_string(),
        blocking: None,
        next_steps: vec![step(
            "Continue work",
            "ralph run resume",
            "No recovery write is required before continuing.",
        )],
    }
}

fn continuation_for_valid_queue(
    blocking: Option<BlockingState>,
    warnings: &[MachineValidationWarning],
) -> MachineContinuationSummary {
    if let Some(blocking) = blocking {
        return MachineContinuationSummary {
            headline: match blocking.status {
                BlockingStatus::Waiting => "Queue continuation is waiting.".to_string(),
                BlockingStatus::Blocked => "Queue continuation is blocked.".to_string(),
                BlockingStatus::Stalled => "Queue continuation is stalled.".to_string(),
            },
            detail: if warnings.is_empty() {
                "The queue structure is valid, but Ralph cannot continue immediately from the current runnability state.".to_string()
            } else {
                "The queue structure is valid, but warnings and current runnability should be reviewed before continuing.".to_string()
            },
            blocking: Some(blocking),
            next_steps: vec![
                step(
                    "Inspect runnability",
                    "ralph queue explain",
                    "Review why the next task is waiting or blocked.",
                ),
                step(
                    "Continue when ready",
                    "ralph run resume",
                    "Resume normal task flow once the queue becomes runnable.",
                ),
            ],
        };
    }

    if warnings.is_empty() {
        return MachineContinuationSummary {
            headline: "Queue continuation is ready.".to_string(),
            detail: "Ralph can continue from the current queue state without queue-level repair."
                .to_string(),
            blocking: None,
            next_steps: vec![
                step(
                    "Continue work",
                    "ralph run resume",
                    "Resume or continue normal task flow.",
                ),
                step(
                    "Preview additional task changes",
                    "ralph task mutate --dry-run",
                    "Preview structured queue changes before writing.",
                ),
            ],
        };
    }

    MachineContinuationSummary {
        headline: "Queue continuation is ready with warnings.".to_string(),
        detail: "Warnings did not invalidate the queue, but they should be reviewed before larger follow-up changes.".to_string(),
        blocking: None,
        next_steps: vec![
            step(
                "Review warnings",
                "ralph queue validate",
                "Inspect the warning list in the human CLI.",
            ),
            step(
                "Continue carefully",
                "ralph run resume",
                "Proceed once the warnings are understood.",
            ),
        ],
    }
}

fn step(title: &str, command: &str, detail: &str) -> MachineContinuationAction {
    MachineContinuationAction {
        title: title.to_string(),
        command: command.to_string(),
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests;
