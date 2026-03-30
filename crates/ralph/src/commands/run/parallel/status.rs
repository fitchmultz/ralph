//! Purpose: parallel status command entrypoint and machine-document builder.
//! Responsibilities: handle `ralph run parallel status` output and build machine-readable documents.
//! Scope: operator-facing status output only.
//! Usage: invoked through `crate::commands::run` re-exports.
//! Not handled here: worker inspection summaries (`status_support.rs`) and table rendering (`status_render.rs`).
//! Invariants/assumptions: callers resolve the repository from CWD; read paths never mutate parallel state.

use super::status_render::print_status_table;
use super::status_support::{inspect_parallel_summaries, lifecycle_counts, step};
use crate::commands::run::parallel::state::{ParallelStateFile, load_state, state_file_path};
use crate::commands::run::queue_lock::{
    QueueLockCondition, QueueLockInspection, inspect_queue_lock,
};
use crate::contracts::{
    BlockingState, BlockingStatus, MACHINE_PARALLEL_STATUS_VERSION, MachineContinuationSummary,
    MachineParallelStatusDocument,
};
use anyhow::{Context, Result};
use serde_json::json;
use std::path::Path;

pub fn parallel_status(resolved: &crate::config::Resolved, json: bool) -> Result<()> {
    let state_path = state_file_path(&resolved.repo_root);

    let state_opt = load_state(&state_path).with_context(|| {
        format!(
            "Failed to load parallel state from {}",
            state_path.display()
        )
    })?;

    let document = build_parallel_status_document(&resolved.repo_root, state_opt.as_ref())?;

    if json {
        let json_str = serde_json::to_string_pretty(&document)
            .context("Failed to serialize status to JSON")?;
        println!("{json_str}");
    } else {
        print_status_table(
            state_opt.as_ref(),
            &document.continuation,
            document.blocking.as_ref(),
        );
    }

    Ok(())
}

pub(crate) fn build_parallel_status_document(
    repo_root: &Path,
    state: Option<&ParallelStateFile>,
) -> Result<MachineParallelStatusDocument> {
    let status = match state {
        Some(state) => serde_json::to_value(state).context("serialize parallel state")?,
        None => json!({
            "schema_version": 3,
            "workers": [],
            "message": "No parallel state found",
        }),
    };
    let queue_lock = inspect_queue_lock(repo_root);
    let (blocking, continuation) = build_parallel_status_guidance(state, queue_lock.as_ref());

    Ok(MachineParallelStatusDocument {
        version: MACHINE_PARALLEL_STATUS_VERSION,
        blocking,
        continuation,
        status,
    })
}

fn build_parallel_status_guidance(
    state: Option<&ParallelStateFile>,
    queue_lock: Option<&QueueLockInspection>,
) -> (Option<BlockingState>, MachineContinuationSummary) {
    if let Some(lock) = queue_lock.filter(|lock| should_surface_parallel_queue_lock(lock, state)) {
        return build_parallel_queue_lock_guidance(lock);
    }

    match state {
        None => (
            None,
            MachineContinuationSummary {
                headline: "Parallel execution has not started.".to_string(),
                detail: "No persisted parallel state was found for this repository. Start a coordinator run to create worker state and begin parallel execution.".to_string(),
                blocking: None,
                next_steps: vec![
                    step(
                        "Start parallel execution",
                        "ralph run loop --parallel <N>",
                        "Start the coordinator with the desired worker count.",
                    ),
                    step(
                        "Inspect status again",
                        "ralph run parallel status",
                        "Re-check worker state after the coordinator starts.",
                    ),
                ],
            },
        ),
        Some(state) => {
            let counts = lifecycle_counts(state);
            let (artifacts, outcomes) = inspect_parallel_summaries(state);
            if counts.has_active() {
                let detail = outcomes.append_to_detail(artifacts.append_to_detail(format!(
                    "Parallel workers are active on target branch {}. running={}, integrating={}, completed={}, failed={}, blocked={}.",
                    state.target_branch,
                    counts.running,
                    counts.integrating,
                    counts.completed,
                    counts.failed,
                    counts.blocked,
                )));
                (
                    None,
                    MachineContinuationSummary {
                        headline: "Parallel execution is in progress.".to_string(),
                        detail,
                        blocking: None,
                        next_steps: vec![step(
                            "Inspect the structured worker snapshot",
                            "ralph run parallel status --json",
                            "Review lifecycle counts, integration outcomes, and retained worker details without scraping logs.",
                        )],
                    },
                )
            } else if counts.blocked > 0 {
                let detail = outcomes.append_to_detail(artifacts.append_to_detail(format!(
                    "{} blocked worker(s) are being skipped until you retry them. completed={}, failed={}.",
                    counts.blocked, counts.completed, counts.failed,
                )));
                let blocking = BlockingState::operator_recovery(
                    BlockingStatus::Blocked,
                    "parallel",
                    "blocked_push",
                    None,
                    "Parallel execution is blocked on worker integration outcomes that need operator action.",
                    detail.clone(),
                    Some("ralph run parallel retry --task <TASK_ID>".to_string()),
                );
                (
                    Some(blocking.clone()),
                    MachineContinuationSummary {
                        headline: "Parallel execution is blocked on worker integration.".to_string(),
                        detail: outcomes.append_to_detail(artifacts.append_to_detail("No workers are actively progressing. Retry each blocked worker after resolving the underlying push, conflict, CI, or validation issue.")),
                        blocking: Some(blocking),
                        next_steps: vec![
                            step(
                                "Inspect blocked integration outcomes",
                                "ralph run parallel status --json",
                                "Check the retained worker reasons, workspace paths, and attempt counts.",
                            ),
                            step(
                                "Retry one blocked worker",
                                "ralph run parallel retry --task <TASK_ID>",
                                "Mark a blocked worker ready for the next coordinator run.",
                            ),
                            step(
                                "Resume the coordinator",
                                "ralph run loop --parallel <N>",
                                "Continue parallel execution after marking workers for retry.",
                            ),
                        ],
                    },
                )
            } else if counts.failed > 0 {
                let detail = outcomes.append_to_detail(artifacts.append_to_detail(format!(
                    "{} worker(s) failed without active progress. completed={}. Inspect the failure reason before retrying.",
                    counts.failed, counts.completed,
                )));
                let blocking = BlockingState::operator_recovery(
                    BlockingStatus::Stalled,
                    "parallel",
                    "worker_failed",
                    None,
                    "Parallel execution is stalled on retryable worker failures.",
                    detail.clone(),
                    Some("ralph run parallel retry --task <TASK_ID>".to_string()),
                );
                (
                    Some(blocking.clone()),
                    MachineContinuationSummary {
                        headline: "Parallel execution needs operator attention.".to_string(),
                        detail: outcomes.append_to_detail(artifacts.append_to_detail("No workers are currently running. Review the retryable worker failures, then retry the affected task when the underlying issue is fixed.")),
                        blocking: Some(blocking),
                        next_steps: vec![
                            step(
                                "Inspect retryable failures",
                                "ralph run parallel status --json",
                                "Review the stored failure reasons and any unexpected retained artifacts before retrying.",
                            ),
                            step(
                                "Retry one failed worker",
                                "ralph run parallel retry --task <TASK_ID>",
                                "Mark the failed worker ready for another coordinator run.",
                            ),
                        ],
                    },
                )
            } else if artifacts.has_cleanup_drift() {
                (
                    None,
                    MachineContinuationSummary {
                        headline: "Parallel execution is idle with cleanup drift.".to_string(),
                        detail: outcomes.append_to_detail(artifacts.append_to_detail(
                            "No workers are active, blocked, or failed, but terminal-worker cleanup drift remains in the retained runtime artifacts.",
                        )),
                        blocking: None,
                        next_steps: vec![
                            step(
                                "Inspect retained artifact paths",
                                "ralph run parallel status --json",
                                "Review which worker workspaces or bookkeeping files were left behind.",
                            ),
                            step(
                                "Resume the coordinator after cleanup",
                                "ralph run loop --parallel <N>",
                                "Restart parallel execution once the retained artifacts match the reported worker state.",
                            ),
                        ],
                    },
                )
            } else {
                (
                    None,
                    MachineContinuationSummary {
                        headline: "Parallel execution is idle.".to_string(),
                        detail: outcomes.append_to_detail(artifacts.append_to_detail(format!(
                            "No workers are active, blocked, or failed. tracked workers: total={}, completed={}. Start another coordinator run if the queue still has pending work.",
                            counts.total, counts.completed,
                        ))),
                        blocking: None,
                        next_steps: vec![step(
                            "Resume the coordinator",
                            "ralph run loop --parallel <N>",
                            "Start another coordinator pass if the queue still contains runnable work.",
                        )],
                    },
                )
            }
        }
    }
}

fn should_surface_parallel_queue_lock(
    lock: &QueueLockInspection,
    state: Option<&ParallelStateFile>,
) -> bool {
    if lock.is_stale_or_unclear() {
        return true;
    }

    !state.is_some_and(|state| lifecycle_counts(state).has_active())
}

fn build_parallel_queue_lock_guidance(
    lock: &QueueLockInspection,
) -> (Option<BlockingState>, MachineContinuationSummary) {
    let blocking = lock.blocking_state.clone();

    let (headline, detail, next_steps) = match lock.condition {
        QueueLockCondition::Live => (
            "Parallel execution is stalled on queue lock contention.",
            "Another Ralph process currently owns the coordinator queue lock. Wait for it to finish, or clear a verified stale lock before restarting the coordinator.",
            vec![
                step(
                    "Inspect the current lock owner",
                    "ralph doctor report",
                    "Confirm which Ralph process owns the queue lock and whether it is still healthy.",
                ),
                step(
                    "Resume the coordinator after the lock clears",
                    "ralph run loop --parallel <N>",
                    "Retry the coordinator once the other Ralph process has finished.",
                ),
            ],
        ),
        QueueLockCondition::Stale => (
            "Parallel execution is stalled on queue lock recovery.",
            "A dead Ralph process left the coordinator queue lock behind. Clear the stale lock before restarting the coordinator.",
            vec![
                step(
                    "Clear the verified stale lock",
                    "ralph queue unlock",
                    "Remove the stale queue lock after confirming the recorded PID is no longer running.",
                ),
                step(
                    "Resume and auto-clear stale ownership",
                    "ralph run loop --parallel <N> --force",
                    "Let the coordinator clear a dead-PID lock and continue in one step.",
                ),
                step(
                    "Confirm the lock state is gone",
                    "ralph run parallel status --json",
                    "Re-check the blocking state before continuing other recovery work.",
                ),
            ],
        ),
        QueueLockCondition::OwnerMissing | QueueLockCondition::OwnerUnreadable => (
            "Parallel execution is stalled on queue lock metadata recovery.",
            "The coordinator queue lock exists, but its owner metadata is incomplete. Verify no other Ralph process is active before clearing it.",
            vec![
                step(
                    "Inspect lock health",
                    "ralph doctor report",
                    "Check whether doctor also sees the queue lock as active or orphaned.",
                ),
                step(
                    "Clear the broken lock record",
                    "ralph queue unlock",
                    "Remove the queue lock after confirming no other Ralph process is running.",
                ),
                step(
                    "Resume the coordinator",
                    "ralph run loop --parallel <N>",
                    "Restart parallel execution after the lock record is cleaned up.",
                ),
            ],
        ),
    };

    (
        Some(blocking.clone()),
        MachineContinuationSummary {
            headline: headline.to_string(),
            detail: detail.to_string(),
            blocking: Some(blocking),
            next_steps,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::run::parallel::{BLOCKED_PUSH_MARKER_FILE, state::WorkerRecord};
    use anyhow::Result;
    use tempfile::TempDir;

    fn blocked_marker_json(task_id: &str, attempt: u32, max_attempts: u32) -> String {
        serde_json::json!({
            "task_id": task_id,
            "reason": "push rejected after conflict review",
            "attempt": attempt,
            "max_attempts": max_attempts,
            "generated_at": "2026-03-22T00:00:00Z"
        })
        .to_string()
    }

    #[test]
    fn parallel_status_describes_retained_blocked_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join(".ralph/workspaces/RQ-1001");
        std::fs::create_dir_all(workspace_path.join(".ralph/cache/parallel"))?;
        std::fs::write(
            workspace_path.join(BLOCKED_PUSH_MARKER_FILE),
            blocked_marker_json("RQ-1001", 3, 5),
        )?;

        let mut state = ParallelStateFile::new("2026-03-21T12:00:00Z", "main");
        let mut worker = WorkerRecord::new(
            "RQ-1001",
            workspace_path.clone(),
            "2026-03-21T12:00:00Z".to_string(),
        );
        worker.mark_blocked(
            "2026-03-21T12:05:00Z".to_string(),
            "push rejected after conflict review",
        );
        worker.push_attempts = 3;
        state.upsert_worker(worker);

        let document = build_parallel_status_document(temp.path(), Some(&state))?;
        assert_eq!(
            document.blocking.as_ref().map(|state| state.status),
            Some(BlockingStatus::Blocked)
        );
        assert!(
            document
                .continuation
                .detail
                .contains("Retained for recovery:")
        );
        assert!(
            document
                .continuation
                .detail
                .contains("Operator action required:")
        );
        assert!(
            document
                .continuation
                .detail
                .contains(&workspace_path.display().to_string())
        );
        assert!(document.continuation.detail.contains("blocked marker 3/5"));
        assert!(
            document
                .continuation
                .detail
                .contains("push rejected after conflict review")
        );
        Ok(())
    }

    #[test]
    fn parallel_status_distinguishes_success_failure_and_action_required() -> Result<()> {
        let temp = TempDir::new()?;
        let blocked_workspace = temp.path().join(".ralph/workspaces/RQ-3003");
        std::fs::create_dir_all(blocked_workspace.join(".ralph/cache/parallel"))?;
        std::fs::write(
            blocked_workspace.join(BLOCKED_PUSH_MARKER_FILE),
            blocked_marker_json("RQ-3003", 2, 5),
        )?;

        let mut state = ParallelStateFile::new("2026-03-21T12:00:00Z", "main");

        let mut completed = WorkerRecord::new(
            "RQ-3001",
            temp.path().join(".ralph/workspaces/RQ-3001"),
            "2026-03-21T12:00:00Z".to_string(),
        );
        completed.mark_completed("2026-03-21T12:10:00Z".to_string());
        state.upsert_worker(completed);

        let mut failed = WorkerRecord::new(
            "RQ-3002",
            temp.path().join(".ralph/workspaces/RQ-3002"),
            "2026-03-21T12:00:00Z".to_string(),
        );
        failed.mark_failed(
            "2026-03-21T12:08:00Z".to_string(),
            "worker exited with status: 1",
        );
        state.upsert_worker(failed);

        let mut blocked = WorkerRecord::new(
            "RQ-3003",
            blocked_workspace,
            "2026-03-21T12:00:00Z".to_string(),
        );
        blocked.mark_blocked(
            "2026-03-21T12:09:00Z".to_string(),
            "push rejected after conflict review",
        );
        blocked.push_attempts = 2;
        state.upsert_worker(blocked);

        let document = build_parallel_status_document(temp.path(), Some(&state))?;
        assert!(
            document
                .continuation
                .detail
                .contains("Integrated successfully:")
        );
        assert!(document.continuation.detail.contains("Retryable failures:"));
        assert!(
            document
                .continuation
                .detail
                .contains("Operator action required:")
        );
        Ok(())
    }

    #[test]
    fn parallel_status_surfaces_cleanup_drift_without_active_workers() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join(".ralph/workspaces/RQ-2001");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state = ParallelStateFile::new("2026-03-21T12:00:00Z", "main");
        let mut worker = WorkerRecord::new(
            "RQ-2001",
            workspace_path.clone(),
            "2026-03-21T12:00:00Z".to_string(),
        );
        worker.mark_completed("2026-03-21T12:05:00Z".to_string());
        state.upsert_worker(worker);

        let document = build_parallel_status_document(temp.path(), Some(&state))?;
        assert!(document.blocking.is_none());
        assert!(document.continuation.headline.contains("cleanup drift"));
        assert!(
            document
                .continuation
                .detail
                .contains("workspace cleanup left")
        );
        assert!(
            document
                .continuation
                .detail
                .contains(&workspace_path.display().to_string())
        );
        Ok(())
    }
}
