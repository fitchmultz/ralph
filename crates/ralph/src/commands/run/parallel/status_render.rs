//! Purpose: human-readable rendering for parallel status output.
//! Responsibilities: render the table-style view and group worker records by lifecycle.
//! Scope: operator-facing console output only.
//! Usage: called from `parallel_ops::status::parallel_status`.
//! Not handled here: CLI dispatch, worker inspection aggregation, or retry mutation flow.
//! Invariants/assumptions: rendering is read-only and output remains stable enough for human operators.

use super::status_support::{
    ParallelArtifactSummary, blocking_status_label, inspect_parallel_summaries, lifecycle_counts,
};
use crate::commands::run::parallel::state::{ParallelStateFile, WorkerLifecycle, WorkerRecord};
use crate::contracts::{BlockingState, MachineContinuationSummary};
use std::collections::HashMap;

pub(crate) fn print_status_table(
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
                let (artifacts, _) = inspect_parallel_summaries(state);
                print_artifact_summary(&artifacts);
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
        println!("Integrated Successfully:");
        for worker in completed {
            println!(
                "  {} - reached {} at {}",
                worker.task_id,
                state.target_branch,
                worker.completed_at.as_deref().unwrap_or("unknown")
            );
        }
        println!();
    }

    if let Some(failed) = by_lifecycle.get(&WorkerLifecycle::Failed) {
        println!("Retryable Failures:");
        for worker in failed {
            println!(
                "  {} - {}",
                worker.task_id,
                worker
                    .last_error
                    .as_deref()
                    .unwrap_or("no recorded failure reason")
            );
        }
        println!();
    }

    if let Some(blocked) = by_lifecycle.get(&WorkerLifecycle::BlockedPush) {
        println!("Operator Action Required:");
        for worker in blocked {
            println!(
                "  {} - {} ({} attempts)",
                worker.task_id,
                worker
                    .last_error
                    .as_deref()
                    .unwrap_or("no recorded block reason"),
                worker.push_attempts
            );
        }
    }
}
