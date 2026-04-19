//! Queue continuation helper builders shared across machine queue documents.
//!
//! Responsibilities:
//! - Build stable blocking and continuation payload fragments for validate/repair flows.
//! - Keep command/action text centralized so machine documents stay consistent.
//!
//! Does not handle:
//! - Queue IO, locking, repair execution, or undo restoration.
//! - CLI routing or JSON serialization.
//!
//! Assumptions/invariants:
//! - Returned actions remain deterministic and version-safe for machine consumers.
//! - Helpers stay focused on shared document text, not business logic branching elsewhere.

use crate::contracts::{
    BlockingState, BlockingStatus, MachineContinuationAction, MachineContinuationSummary,
    MachineValidationWarning,
};

pub(super) fn queue_validation_failed_state(detail: String) -> BlockingState {
    BlockingState::operator_recovery(
        BlockingStatus::Stalled,
        "queue_validate",
        "validation_failed",
        None,
        "Ralph is stalled on queue consistency.",
        detail,
        Some("ralph queue repair --dry-run".to_string()),
    )
    .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback())
}

pub(super) fn repair_preview_continuation(changed: bool) -> MachineContinuationSummary {
    if changed {
        return MachineContinuationSummary {
            headline: "Repair preview is ready.".to_string(),
            detail: "Ralph found recoverable queue issues and can preserve the current queue by normalizing them.".to_string(),
            blocking: Some(
                BlockingState::operator_recovery(
                    BlockingStatus::Stalled,
                    "queue_repair",
                    "repair_available",
                    None,
                    "Ralph found recoverable queue issues.",
                    "Preview completed successfully; apply the repair to continue from a normalized queue state.",
                    Some("ralph queue repair".to_string()),
                )
                .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
            ),
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

pub(super) fn continuation_for_valid_queue(
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

pub(super) fn step(title: &str, command: &str, detail: &str) -> MachineContinuationAction {
    MachineContinuationAction {
        title: title.to_string(),
        command: command.to_string(),
        detail: detail.to_string(),
    }
}
