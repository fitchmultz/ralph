//! Purpose: parallel status inspection helpers.
//! Responsibilities: inspect worker workspaces, blocked-push markers, and lifecycle summaries.
//! Scope: read-only status aggregation for `ralph run parallel status`.
//! Usage: called from `parallel_ops` status/document rendering code.
//! Not handled here: CLI dispatch, table rendering, or retry mutation flow.
//! Invariants/assumptions: reads workspace-local runtime artifacts only and never mutates queue or worker state.

use crate::commands::run::parallel::{
    BLOCKED_PUSH_MARKER_FILE, read_blocked_push_marker,
    state::{ParallelStateFile, WorkerLifecycle, WorkerRecord},
};
use crate::contracts::{BlockingStatus, MachineContinuationAction};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParallelLifecycleCounts {
    pub(crate) total: usize,
    pub(crate) running: usize,
    pub(crate) integrating: usize,
    pub(crate) completed: usize,
    pub(crate) failed: usize,
    pub(crate) blocked: usize,
}

impl ParallelLifecycleCounts {
    pub(crate) fn has_active(self) -> bool {
        self.running > 0 || self.integrating > 0
    }
}

#[derive(Debug, Default)]
pub(crate) struct ParallelArtifactSummary {
    pub(crate) retained_for_recovery: Vec<String>,
    pub(crate) cleanup_drift: Vec<String>,
}

impl ParallelArtifactSummary {
    pub(crate) fn has_retained_for_recovery(&self) -> bool {
        !self.retained_for_recovery.is_empty()
    }

    pub(crate) fn has_cleanup_drift(&self) -> bool {
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

    pub(crate) fn append_to_detail(&self, base: impl Into<String>) -> String {
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

#[derive(Debug, Default)]
pub(crate) struct ParallelOutcomeSummary {
    integrated_successfully: Vec<String>,
    retryable_failures: Vec<String>,
    operator_action_required: Vec<String>,
}

impl ParallelOutcomeSummary {
    fn integrated_sentence(&self) -> Option<String> {
        if self.integrated_successfully.is_empty() {
            return None;
        }

        Some(format!(
            "Integrated successfully: {}.",
            self.integrated_successfully.join("; ")
        ))
    }

    fn retryable_failure_sentence(&self) -> Option<String> {
        if self.retryable_failures.is_empty() {
            return None;
        }

        Some(format!(
            "Retryable failures: {}.",
            self.retryable_failures.join("; ")
        ))
    }

    fn operator_action_sentence(&self) -> Option<String> {
        if self.operator_action_required.is_empty() {
            return None;
        }

        Some(format!(
            "Operator action required: {}.",
            self.operator_action_required.join("; ")
        ))
    }

    pub(crate) fn append_to_detail(&self, base: impl Into<String>) -> String {
        let mut parts = vec![base.into()];
        if let Some(successes) = self.integrated_sentence() {
            parts.push(successes);
        }
        if let Some(failures) = self.retryable_failure_sentence() {
            parts.push(failures);
        }
        if let Some(actions) = self.operator_action_sentence() {
            parts.push(actions);
        }
        parts.join(" ")
    }
}

pub(crate) fn lifecycle_counts(state: &ParallelStateFile) -> ParallelLifecycleCounts {
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

enum WorkerBlockedMarker {
    Missing,
    Parsed {
        attempt: u32,
        max_attempts: u32,
        reason: String,
    },
    Unreadable {
        error: String,
    },
}

struct WorkerStatusInspection<'a> {
    worker: &'a WorkerRecord,
    workspace_exists: bool,
    marker_path: PathBuf,
    marker: WorkerBlockedMarker,
}

pub(crate) fn inspect_parallel_summaries(
    state: &ParallelStateFile,
) -> (ParallelArtifactSummary, ParallelOutcomeSummary) {
    let mut artifacts = ParallelArtifactSummary::default();
    let mut outcomes = ParallelOutcomeSummary::default();

    for worker in &state.workers {
        let inspected = inspect_worker_status(worker);

        match inspected.worker.lifecycle {
            WorkerLifecycle::Completed => {
                outcomes.integrated_successfully.push(format!(
                    "{} reached {} at {}",
                    inspected.worker.task_id,
                    state.target_branch,
                    inspected
                        .worker
                        .completed_at
                        .as_deref()
                        .unwrap_or("unknown completion time"),
                ));
                if inspected.workspace_exists {
                    artifacts.cleanup_drift.push(format!(
                        "{} is {} but workspace cleanup left {} behind",
                        inspected.worker.task_id,
                        lifecycle_label(&inspected.worker.lifecycle),
                        inspected.worker.workspace_path.display(),
                    ));
                }
                if !matches!(inspected.marker, WorkerBlockedMarker::Missing) {
                    artifacts.cleanup_drift.push(format!(
                        "{} is {} but blocked marker cleanup left {} behind",
                        inspected.worker.task_id,
                        lifecycle_label(&inspected.worker.lifecycle),
                        inspected.marker_path.display(),
                    ));
                }
            }
            WorkerLifecycle::Failed => {
                outcomes.retryable_failures.push(format!(
                    "{} stopped with {}",
                    inspected.worker.task_id,
                    inspected
                        .worker
                        .last_error
                        .as_deref()
                        .unwrap_or("no recorded failure reason"),
                ));
                if inspected.workspace_exists {
                    artifacts.cleanup_drift.push(format!(
                        "{} is {} but workspace cleanup left {} behind",
                        inspected.worker.task_id,
                        lifecycle_label(&inspected.worker.lifecycle),
                        inspected.worker.workspace_path.display(),
                    ));
                }
                if !matches!(inspected.marker, WorkerBlockedMarker::Missing) {
                    artifacts.cleanup_drift.push(format!(
                        "{} is {} but blocked marker cleanup left {} behind",
                        inspected.worker.task_id,
                        lifecycle_label(&inspected.worker.lifecycle),
                        inspected.marker_path.display(),
                    ));
                }
            }
            WorkerLifecycle::BlockedPush => {
                if inspected.workspace_exists {
                    let mut retained = format!(
                        "{} keeps {}",
                        inspected.worker.task_id,
                        inspected.worker.workspace_path.display(),
                    );
                    match &inspected.marker {
                        WorkerBlockedMarker::Parsed {
                            attempt,
                            max_attempts,
                            ..
                        } => {
                            retained.push_str(&format!(
                                " with blocked marker {}/{}",
                                attempt, max_attempts,
                            ));
                        }
                        WorkerBlockedMarker::Unreadable { .. } => {
                            retained.push_str(" with unreadable blocked marker");
                        }
                        WorkerBlockedMarker::Missing => {
                            retained.push_str(" without a blocked marker file");
                        }
                    }
                    artifacts.retained_for_recovery.push(retained);
                } else {
                    artifacts.cleanup_drift.push(format!(
                        "{} is blocked for retry but its workspace is missing ({})",
                        inspected.worker.task_id,
                        inspected.worker.workspace_path.display(),
                    ));
                }

                match &inspected.marker {
                    WorkerBlockedMarker::Parsed {
                        attempt,
                        max_attempts,
                        reason,
                    } => {
                        outcomes.operator_action_required.push(format!(
                            "{} blocked after {}/{} attempts ({})",
                            inspected.worker.task_id, attempt, max_attempts, reason,
                        ));
                    }
                    WorkerBlockedMarker::Missing => {
                        outcomes.operator_action_required.push(format!(
                            "{} is retained for operator recovery ({})",
                            inspected.worker.task_id,
                            inspected
                                .worker
                                .last_error
                                .as_deref()
                                .unwrap_or("no recorded block reason"),
                        ));
                    }
                    WorkerBlockedMarker::Unreadable { error } => {
                        artifacts.cleanup_drift.push(format!(
                            "{} has an unreadable blocked marker at {} ({error})",
                            inspected.worker.task_id,
                            inspected.marker_path.display(),
                        ));
                        outcomes.operator_action_required.push(format!(
                            "{} is retained for operator recovery but its blocked marker is unreadable ({error})",
                            inspected.worker.task_id,
                        ));
                    }
                }
            }
            WorkerLifecycle::Running | WorkerLifecycle::Integrating => {}
        }
    }

    (artifacts, outcomes)
}

fn inspect_worker_status(worker: &WorkerRecord) -> WorkerStatusInspection<'_> {
    let marker_path = worker.workspace_path.join(BLOCKED_PUSH_MARKER_FILE);
    let marker = match read_blocked_push_marker(&worker.workspace_path) {
        Ok(Some(marker)) => WorkerBlockedMarker::Parsed {
            attempt: marker.attempt,
            max_attempts: marker.max_attempts,
            reason: marker.reason,
        },
        Ok(None) => WorkerBlockedMarker::Missing,
        Err(err) => WorkerBlockedMarker::Unreadable {
            error: err.to_string(),
        },
    };

    WorkerStatusInspection {
        worker,
        workspace_exists: worker.workspace_path.exists(),
        marker_path,
        marker,
    }
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

pub(crate) fn step(title: &str, command: &str, detail: &str) -> MachineContinuationAction {
    MachineContinuationAction {
        title: title.to_string(),
        command: command.to_string(),
        detail: detail.to_string(),
    }
}

pub(crate) fn blocking_status_label(status: &BlockingStatus) -> &'static str {
    match status {
        BlockingStatus::Waiting => "waiting",
        BlockingStatus::Blocked => "blocked",
        BlockingStatus::Stalled => "stalled",
    }
}
