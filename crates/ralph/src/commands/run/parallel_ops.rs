//! Parallel operations commands (status, retry) for CLI.
//!
//! Responsibilities:
//! - Implement `ralph run parallel status` to show worker states.
//! - Implement `ralph run parallel retry` to resume blocked workers.
//!
//! Not handled here:
//! - Worker orchestration (see `parallel/orchestration.rs`).
//! - Integration loop logic (see `parallel/integration.rs`).
//!
//! Invariants/assumptions:
//! - Commands run in coordinator repo context (CWD is repo root).
//! - State file is at `.ralph/cache/parallel/state.json`.

use crate::commands::run::parallel::{
    BLOCKED_PUSH_MARKER_FILE, read_blocked_push_marker,
    state::{
        ParallelStateFile, WorkerLifecycle, WorkerRecord, load_state, save_state, state_file_path,
    },
};
use crate::commands::run::queue_lock::{
    QueueLockCondition, QueueLockInspection, inspect_queue_lock,
};
use crate::contracts::{
    BlockingState, BlockingStatus, MACHINE_PARALLEL_STATUS_VERSION, MachineContinuationAction,
    MachineContinuationSummary, MachineParallelStatusDocument,
};
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
struct ParallelLifecycleCounts {
    total: usize,
    running: usize,
    integrating: usize,
    completed: usize,
    failed: usize,
    blocked: usize,
}

impl ParallelLifecycleCounts {
    fn has_active(self) -> bool {
        self.running > 0 || self.integrating > 0
    }
}

#[derive(Debug, Default)]
struct ParallelArtifactSummary {
    retained_for_recovery: Vec<String>,
    cleanup_drift: Vec<String>,
}

impl ParallelArtifactSummary {
    fn has_retained_for_recovery(&self) -> bool {
        !self.retained_for_recovery.is_empty()
    }

    fn has_cleanup_drift(&self) -> bool {
        !self.cleanup_drift.is_empty()
    }

    fn retention_sentence(&self) -> Option<String> {
        if !self.has_retained_for_recovery() {
            return None;
        }

        Some(format!(
            "Retained for recovery: {}.",
            self.retained_for_recovery.join("; ")
        ))
    }

    fn cleanup_drift_sentence(&self) -> Option<String> {
        if !self.has_cleanup_drift() {
            return None;
        }

        Some(format!("Cleanup drift: {}.", self.cleanup_drift.join("; ")))
    }

    fn append_to_detail(&self, base: impl Into<String>) -> String {
        let mut parts = vec![base.into()];
        if let Some(retention) = self.retention_sentence() {
            parts.push(retention);
        }
        if let Some(drift) = self.cleanup_drift_sentence() {
            parts.push(drift);
        }
        parts.join(" ")
    }
}

/// Show status of parallel workers.
///
/// If `json` is true, outputs structured JSON. Otherwise, prints human-readable table.
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
            let artifacts = inspect_parallel_artifacts(state);
            if counts.has_active() {
                let detail = artifacts.append_to_detail(format!(
                    "Parallel workers are active on target branch {}. running={}, integrating={}, completed={}, failed={}, blocked={}.",
                    state.target_branch,
                    counts.running,
                    counts.integrating,
                    counts.completed,
                    counts.failed,
                    counts.blocked,
                ));
                (
                    None,
                    MachineContinuationSummary {
                        headline: "Parallel execution is in progress.".to_string(),
                        detail,
                        blocking: None,
                        next_steps: vec![step(
                            "Inspect the structured worker snapshot",
                            "ralph run parallel status --json",
                            "Review lifecycle counts and retained worker details without scraping logs.",
                        )],
                    },
                )
            } else if counts.blocked > 0 {
                let detail = artifacts.append_to_detail(format!(
                    "{} blocked worker(s) are being skipped until you retry them. completed={}, failed={}.",
                    counts.blocked, counts.completed, counts.failed,
                ));
                let blocking = BlockingState::operator_recovery(
                    BlockingStatus::Blocked,
                    "parallel",
                    "blocked_push",
                    None,
                    "Parallel execution is blocked on retained worker pushes.",
                    detail.clone(),
                    Some("ralph run parallel retry --task <TASK_ID>".to_string()),
                );
                (
                    Some(blocking.clone()),
                    MachineContinuationSummary {
                        headline: "Parallel execution is blocked on worker integration.".to_string(),
                        detail: artifacts.append_to_detail("No workers are actively progressing. Retry each blocked worker after resolving the underlying push, conflict, or CI issue."),
                        blocking: Some(blocking),
                        next_steps: vec![
                            step(
                                "Inspect blocked workers",
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
                let detail = artifacts.append_to_detail(format!(
                    "{} worker(s) failed without active progress. completed={}. Inspect the failure reason before retrying.",
                    counts.failed, counts.completed,
                ));
                let blocking = BlockingState::operator_recovery(
                    BlockingStatus::Stalled,
                    "parallel",
                    "worker_failed",
                    None,
                    "Parallel execution is stalled on worker failure.",
                    detail.clone(),
                    Some("ralph run parallel retry --task <TASK_ID>".to_string()),
                );
                (
                    Some(blocking.clone()),
                    MachineContinuationSummary {
                        headline: "Parallel execution needs operator attention.".to_string(),
                        detail: artifacts.append_to_detail("No workers are currently running. Review the failed worker state, then retry the affected task when the underlying issue is fixed."),
                        blocking: Some(blocking),
                        next_steps: vec![
                            step(
                                "Inspect failed workers",
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
                        detail: artifacts.append_to_detail(
                            "No workers are active, blocked, or failed, but terminal-worker cleanup drift remains in the retained runtime artifacts.",
                        ),
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
                        detail: artifacts.append_to_detail(format!(
                            "No workers are active, blocked, or failed. tracked workers: total={}, completed={}. Start another coordinator run if the queue still has pending work.",
                            counts.total, counts.completed,
                        )),
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

fn lifecycle_counts(state: &ParallelStateFile) -> ParallelLifecycleCounts {
    ParallelLifecycleCounts {
        total: state.workers.len(),
        running: state.workers_by_lifecycle(WorkerLifecycle::Running).count(),
        integrating: state
            .workers_by_lifecycle(WorkerLifecycle::Integrating)
            .count(),
        completed: state
            .workers_by_lifecycle(WorkerLifecycle::Completed)
            .count(),
        failed: state.workers_by_lifecycle(WorkerLifecycle::Failed).count(),
        blocked: state
            .workers_by_lifecycle(WorkerLifecycle::BlockedPush)
            .count(),
    }
}

fn inspect_parallel_artifacts(state: &ParallelStateFile) -> ParallelArtifactSummary {
    let mut summary = ParallelArtifactSummary::default();

    for worker in &state.workers {
        let workspace_exists = worker.workspace_path.exists();
        let marker_path = worker.workspace_path.join(BLOCKED_PUSH_MARKER_FILE);
        let marker_exists = marker_path.exists();
        let marker = match read_blocked_push_marker(&worker.workspace_path) {
            Ok(marker) => marker,
            Err(err) => {
                summary.cleanup_drift.push(format!(
                    "{} has an unreadable blocked marker at {} ({err})",
                    worker.task_id,
                    marker_path.display(),
                ));
                None
            }
        };

        match worker.lifecycle {
            WorkerLifecycle::BlockedPush => {
                if workspace_exists {
                    let mut retained = format!(
                        "{} keeps {}",
                        worker.task_id,
                        worker.workspace_path.display(),
                    );
                    if let Some(marker) = marker.as_ref() {
                        retained.push_str(&format!(
                            " with blocked marker {}/{}",
                            marker.attempt, marker.max_attempts,
                        ));
                    } else if marker_exists {
                        retained.push_str(" with unreadable blocked marker");
                    } else {
                        retained.push_str(" without a blocked marker file");
                    }
                    summary.retained_for_recovery.push(retained);
                } else {
                    summary.cleanup_drift.push(format!(
                        "{} is blocked for retry but its workspace is missing ({})",
                        worker.task_id,
                        worker.workspace_path.display(),
                    ));
                }
            }
            WorkerLifecycle::Completed | WorkerLifecycle::Failed => {
                if workspace_exists {
                    summary.cleanup_drift.push(format!(
                        "{} is {} but workspace cleanup left {} behind",
                        worker.task_id,
                        lifecycle_label(&worker.lifecycle),
                        worker.workspace_path.display(),
                    ));
                }
                if marker_exists {
                    summary.cleanup_drift.push(format!(
                        "{} is {} but blocked marker cleanup left {} behind",
                        worker.task_id,
                        lifecycle_label(&worker.lifecycle),
                        marker_path.display(),
                    ));
                }
            }
            WorkerLifecycle::Running | WorkerLifecycle::Integrating => {}
        }
    }

    summary
}

fn lifecycle_label(lifecycle: &WorkerLifecycle) -> &'static str {
    match lifecycle {
        WorkerLifecycle::Running => "running",
        WorkerLifecycle::Integrating => "integrating",
        WorkerLifecycle::Completed => "completed",
        WorkerLifecycle::Failed => "failed",
        WorkerLifecycle::BlockedPush => "blocked_push",
    }
}

fn step(title: &str, command: &str, detail: &str) -> MachineContinuationAction {
    MachineContinuationAction {
        title: title.to_string(),
        command: command.to_string(),
        detail: detail.to_string(),
    }
}

fn blocking_status_label(status: &BlockingStatus) -> &'static str {
    match status {
        BlockingStatus::Waiting => "waiting",
        BlockingStatus::Blocked => "blocked",
        BlockingStatus::Stalled => "stalled",
    }
}

fn print_status_table(
    state: Option<&ParallelStateFile>,
    continuation: &MachineContinuationSummary,
    blocking: Option<&BlockingState>,
) {
    println!("{}", continuation.headline);
    println!("{}", continuation.detail);

    if let Some(blocking) = blocking {
        println!();
        println!(
            "Operator state: {}",
            blocking_status_label(&blocking.status)
        );
        println!("{}", blocking.message);
        if !blocking.detail.trim().is_empty() {
            println!("{}", blocking.detail);
        }
    }

    println!();
    println!("Parallel Run Status");
    println!("===================");

    match state {
        None => {
            println!("No parallel run state found.");
        }
        Some(state) => {
            println!("Schema Version: {}", state.schema_version);
            println!("Started:        {}", state.started_at);
            println!("Target Branch:  {}", state.target_branch);
            println!();

            if state.workers.is_empty() {
                println!("No workers tracked.");
            } else {
                print_worker_groups(state);
                print_artifact_summary(&inspect_parallel_artifacts(state));
            }
        }
    }

    if !continuation.next_steps.is_empty() {
        println!();
        println!("Next:");
        for (index, next_step) in continuation.next_steps.iter().enumerate() {
            println!(
                "  {}. {} — {}",
                index + 1,
                next_step.command,
                next_step.detail
            );
        }
    }
}

fn print_artifact_summary(summary: &ParallelArtifactSummary) {
    if summary.retained_for_recovery.is_empty() && summary.cleanup_drift.is_empty() {
        return;
    }

    if !summary.retained_for_recovery.is_empty() {
        println!("Retained for Recovery:");
        for line in &summary.retained_for_recovery {
            println!("  {line}");
        }
        println!();
    }

    if !summary.cleanup_drift.is_empty() {
        println!("Cleanup Drift:");
        for line in &summary.cleanup_drift {
            println!("  {line}");
        }
        println!();
    }
}

fn print_worker_groups(state: &ParallelStateFile) {
    let mut by_lifecycle: HashMap<WorkerLifecycle, Vec<&WorkerRecord>> = HashMap::new();
    for worker in &state.workers {
        by_lifecycle
            .entry(worker.lifecycle.clone())
            .or_default()
            .push(worker);
    }

    let counts = lifecycle_counts(state);
    println!(
        "Total: {} | Running: {} | Integrating: {} | Completed: {} | Failed: {} | Blocked: {}",
        counts.total,
        counts.running,
        counts.integrating,
        counts.completed,
        counts.failed,
        counts.blocked,
    );
    println!();

    if let Some(active) = by_lifecycle.get(&WorkerLifecycle::Running) {
        println!("Running Workers:");
        for worker in active {
            println!(
                "  {} - started {} ({} attempts)",
                worker.task_id, worker.started_at, worker.push_attempts
            );
        }
        println!();
    }

    if let Some(integrating) = by_lifecycle.get(&WorkerLifecycle::Integrating) {
        println!("Integrating Workers:");
        for worker in integrating {
            println!(
                "  {} - started {} ({} attempts)",
                worker.task_id, worker.started_at, worker.push_attempts
            );
        }
        println!();
    }

    if let Some(completed) = by_lifecycle.get(&WorkerLifecycle::Completed) {
        println!("Completed Workers:");
        for worker in completed {
            println!(
                "  {} - completed {}",
                worker.task_id,
                worker.completed_at.as_deref().unwrap_or("unknown")
            );
        }
        println!();
    }

    if let Some(failed) = by_lifecycle.get(&WorkerLifecycle::Failed) {
        println!("Failed Workers:");
        for worker in failed {
            println!(
                "  {} - {}",
                worker.task_id,
                worker.last_error.as_deref().unwrap_or("no error")
            );
        }
        println!();
    }

    if let Some(blocked) = by_lifecycle.get(&WorkerLifecycle::BlockedPush) {
        println!("Blocked Workers:");
        for worker in blocked {
            println!(
                "  {} - {} ({} attempts)",
                worker.task_id,
                worker.last_error.as_deref().unwrap_or("blocked"),
                worker.push_attempts
            );
        }
    }
}

/// Retry a blocked or failed parallel worker.
///
/// This resumes the integration loop for a worker that is in a terminal
/// state (BlockedPush or Failed).
pub fn parallel_retry(resolved: &crate::config::Resolved, task_id: &str) -> Result<()> {
    let state_path = state_file_path(&resolved.repo_root);

    let mut state = match load_state(&state_path).with_context(|| {
        format!(
            "Failed to load parallel state from {}",
            state_path.display()
        )
    })? {
        Some(state) => state,
        None => {
            anyhow::bail!("No parallel run state found. Run `ralph run loop --parallel N` first.");
        }
    };

    let worker = state
        .get_worker(task_id)
        .ok_or_else(|| anyhow::anyhow!("Task {} not found in parallel state", task_id))?;

    match worker.lifecycle {
        WorkerLifecycle::BlockedPush | WorkerLifecycle::Failed => {
            let mut updated_worker = worker.clone();
            updated_worker.lifecycle = WorkerLifecycle::Running;
            updated_worker.last_error = None;

            state.upsert_worker(updated_worker);
            save_state(&state_path, &state).context("Failed to save updated worker state")?;

            println!("Parallel retry is ready.");
            println!(
                "Worker {} will be reconsidered the next time the coordinator resumes parallel execution.",
                task_id
            );
            println!();
            println!("Next:");
            println!(
                "  1. ralph run loop --parallel <N> — resume the coordinator so the worker can run again."
            );
            println!(
                "  2. ralph run parallel status — confirm the worker is no longer retained as blocked or failed."
            );

            Ok(())
        }
        WorkerLifecycle::Completed => {
            anyhow::bail!(
                "Task {} has already completed successfully. No retry needed.",
                task_id
            )
        }
        WorkerLifecycle::Running | WorkerLifecycle::Integrating => {
            anyhow::bail!(
                "Task {} is currently {}. Cannot retry an active worker.",
                task_id,
                match worker.lifecycle {
                    WorkerLifecycle::Running => "running",
                    WorkerLifecycle::Integrating => "integrating",
                    _ => unreachable!(),
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
                .contains(&workspace_path.display().to_string())
        );
        assert!(document.continuation.detail.contains("blocked marker 3/5"));
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
