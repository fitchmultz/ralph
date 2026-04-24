//! Machine task continuation document builders.
//!
//! Purpose:
//! - Machine task continuation document builders.
//!
//! Responsibilities:
//! - Build stable continuation summaries for task mutation and decomposition flows.
//! - Keep operator-facing next steps and blocking narration consistent for machine task commands.
//!
//! Not handled here:
//! - Queue/task write orchestration.
//! - Machine JSON envelope assembly.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Continuation text stays deterministic for machine consumers.
//! - Blocking summaries use the shared machine/blocking contract types.

use crate::cli::machine::common::{
    machine_queue_graph_command, machine_queue_undo_dry_run_command,
    machine_queue_validate_command, machine_run_one_resume_command, machine_task_build_command,
    machine_task_decompose_command, machine_task_mutate_command,
};
use crate::commands::task as task_cmd;
use crate::contracts::{
    BlockingState, BlockingStatus, MachineContinuationAction, MachineContinuationSummary,
};

fn step(title: &str, command: &str, detail: &str) -> MachineContinuationAction {
    MachineContinuationAction {
        title: title.to_string(),
        command: command.to_string(),
        detail: detail.to_string(),
    }
}

pub(super) fn build_continuation(created_count: usize) -> MachineContinuationSummary {
    MachineContinuationSummary {
        headline: "AI Build has created task drafts.".to_string(),
        detail: format!(
            "Ralph ran the task-builder prompt and added {created_count} task(s) to the queue."
        ),
        blocking: None,
        next_steps: vec![
            step(
                "Review created tasks",
                machine_queue_graph_command(),
                "Inspect the new task placement, relationships, estimates, and scope.",
            ),
            step(
                "Run the next task",
                machine_run_one_resume_command(),
                "Continue from the updated queue when the task drafts look right.",
            ),
            step(
                "Build another task",
                machine_task_build_command(),
                "Use the same machine contract for another guided AI task creation.",
            ),
        ],
    }
}

pub(super) fn mutation_continuation(
    task_count: usize,
    dry_run: bool,
) -> MachineContinuationSummary {
    if dry_run {
        return MachineContinuationSummary {
            headline: "Mutation continuation is ready.".to_string(),
            detail: format!(
                "Ralph validated an atomic mutation affecting {task_count} task(s) without writing queue changes."
            ),
            blocking: None,
            next_steps: vec![
                step(
                    "Apply the mutation",
                    machine_task_mutate_command(false),
                    "Repeat without --dry-run to write the validated transaction.",
                ),
                step(
                    "Refresh if the queue moved",
                    machine_queue_validate_command(),
                    "Validate and retry from the latest queue state if another write landed first.",
                ),
            ],
        };
    }

    MachineContinuationSummary {
        headline: "Task mutation has been applied.".to_string(),
        detail: format!(
            "Ralph wrote {task_count} task mutation(s) atomically and created an undo checkpoint first."
        ),
        blocking: None,
        next_steps: vec![
            step(
                "Continue work",
                machine_run_one_resume_command(),
                "Proceed from the updated task state.",
            ),
            step(
                "Restore if needed",
                machine_queue_undo_dry_run_command(),
                "Preview the rollback path for this mutation.",
            ),
        ],
    }
}

pub(super) fn decompose_continuation(
    preview: &task_cmd::DecompositionPreview,
    write: Option<&task_cmd::TaskDecomposeWriteResult>,
) -> MachineContinuationSummary {
    if write.is_some() {
        return MachineContinuationSummary {
            headline: "Decomposition has been written.".to_string(),
            detail: "Ralph wrote the planned task tree and created an undo checkpoint before mutating the queue."
                .to_string(),
            blocking: None,
            next_steps: vec![
                step(
                    "Inspect the tree",
                    machine_queue_graph_command(),
                    "Review the written parent/child structure.",
                ),
                step(
                    "Restore if needed",
                    machine_queue_undo_dry_run_command(),
                    "Preview the rollback path for this decomposition.",
                ),
            ],
        };
    }

    if preview.write_blockers.is_empty() {
        return MachineContinuationSummary {
            headline: "Decomposition preview is ready.".to_string(),
            detail: "Ralph planned a task tree that can be written when you are ready.".to_string(),
            blocking: None,
            next_steps: vec![step(
                "Write the preview",
                &machine_task_decompose_command(true, "..."),
                "Persist the planned tree into the queue.",
            )],
        };
    }

    MachineContinuationSummary {
        headline: "Decomposition preview is blocked from being written.".to_string(),
        detail: "Ralph preserved the proposed tree, but a queue invariant must be resolved before write mode can continue.".to_string(),
        blocking: Some(
            BlockingState::operator_recovery(
                BlockingStatus::Blocked,
                "task_decompose",
                "write_blocked",
                preview
                    .attach_target
                    .as_ref()
                    .map(|target| target.task.id.clone()),
                "Ralph is blocked from continuing this decomposition write.",
                preview.write_blockers.join(" "),
                Some(machine_task_decompose_command(true, "--child-policy append ...")),
            )
            .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback()),
        ),
        next_steps: vec![
            step(
                "Append under the existing parent",
                &machine_task_decompose_command(true, "--child-policy append ..."),
                "Keep existing children and add the new subtree.",
            ),
            step(
                "Replace the existing subtree",
                &machine_task_decompose_command(true, "--child-policy replace ..."),
                "Use only when the existing subtree can be safely replaced.",
            ),
            step(
                "Keep the preview only",
                &machine_task_decompose_command(false, "..."),
                "Retain the proposed tree while deciding how to proceed.",
            ),
        ],
    }
}
