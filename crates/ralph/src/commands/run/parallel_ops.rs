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

use crate::commands::run::parallel::state::{load_state, state_file_path};
use anyhow::{Context, Result};
use std::collections::HashMap;

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

    let state = match state_opt {
        Some(s) => s,
        None => {
            if json {
                println!(
                    "{{\"schema_version\":3,\"workers\":[],\"message\":\"No parallel state found\"}}"
                );
            } else {
                println!("No parallel run state found.");
                println!("Run `ralph run loop --parallel N` to start parallel execution.");
            }
            return Ok(());
        }
    };

    if json {
        // Output raw state as JSON
        let json_str =
            serde_json::to_string_pretty(&state).context("Failed to serialize state to JSON")?;
        println!("{}", json_str);
    } else {
        // Human-readable output
        print_status_table(&state);
    }

    Ok(())
}

fn print_status_table(state: &crate::commands::run::parallel::state::ParallelStateFile) {
    use crate::commands::run::parallel::state::WorkerLifecycle;

    println!("Parallel Run Status");
    println!("===================");
    println!("Schema Version: {}", state.schema_version);
    println!("Started:        {}", state.started_at);
    println!("Target Branch:  {}", state.target_branch);
    println!();

    if state.workers.is_empty() {
        println!("No workers tracked.");
        return;
    }

    // Group workers by lifecycle
    let mut by_lifecycle: HashMap<
        WorkerLifecycle,
        Vec<&crate::commands::run::parallel::state::WorkerRecord>,
    > = HashMap::new();
    for worker in &state.workers {
        by_lifecycle
            .entry(worker.lifecycle.clone())
            .or_default()
            .push(worker);
    }

    // Print summary
    println!(
        "Total: {} | Running: {} | Integrating: {} | Completed: {} | Failed: {} | Blocked: {}",
        state.workers.len(),
        by_lifecycle
            .get(&WorkerLifecycle::Running)
            .map_or(0, |v| v.len()),
        by_lifecycle
            .get(&WorkerLifecycle::Integrating)
            .map_or(0, |v| v.len()),
        by_lifecycle
            .get(&WorkerLifecycle::Completed)
            .map_or(0, |v| v.len()),
        by_lifecycle
            .get(&WorkerLifecycle::Failed)
            .map_or(0, |v| v.len()),
        by_lifecycle
            .get(&WorkerLifecycle::BlockedPush)
            .map_or(0, |v| v.len()),
    );
    println!();

    // Print active workers (running, integrating)
    if let Some(active) = by_lifecycle.get(&WorkerLifecycle::Running) {
        println!("Running Workers:");
        for w in active {
            println!(
                "  {} - started {} ({} attempts)",
                w.task_id, w.started_at, w.push_attempts
            );
        }
        println!();
    }

    if let Some(integrating) = by_lifecycle.get(&WorkerLifecycle::Integrating) {
        println!("Integrating Workers:");
        for w in integrating {
            println!(
                "  {} - started {} ({} attempts)",
                w.task_id, w.started_at, w.push_attempts
            );
        }
        println!();
    }

    // Print terminal workers
    if let Some(completed) = by_lifecycle.get(&WorkerLifecycle::Completed) {
        println!("Completed Workers:");
        for w in completed {
            println!(
                "  {} - completed {}",
                w.task_id,
                w.completed_at.as_deref().unwrap_or("unknown")
            );
        }
        println!();
    }

    if let Some(failed) = by_lifecycle.get(&WorkerLifecycle::Failed) {
        println!("Failed Workers:");
        for w in failed {
            println!(
                "  {} - {}",
                w.task_id,
                w.last_error.as_deref().unwrap_or("no error")
            );
        }
        println!();
    }

    if let Some(blocked) = by_lifecycle.get(&WorkerLifecycle::BlockedPush) {
        println!("Blocked Workers (use `ralph run parallel retry --task <ID>`):");
        for w in blocked {
            println!(
                "  {} - {} ({} attempts)",
                w.task_id,
                w.last_error.as_deref().unwrap_or("blocked"),
                w.push_attempts
            );
        }
        println!();
    }
}

/// Retry a blocked or failed parallel worker.
///
/// This resumes the integration loop for a worker that is in a terminal
/// state (BlockedPush or Failed).
pub fn parallel_retry(
    resolved: &crate::config::Resolved,
    task_id: &str,
    _force: bool,
) -> Result<()> {
    use crate::commands::run::parallel::state::{WorkerLifecycle, load_state, save_state};

    let state_path = state_file_path(&resolved.repo_root);

    let mut state = match load_state(&state_path).with_context(|| {
        format!(
            "Failed to load parallel state from {}",
            state_path.display()
        )
    })? {
        Some(s) => s,
        None => {
            anyhow::bail!("No parallel run state found. Run `ralph run loop --parallel N` first.");
        }
    };

    // Find the worker
    let worker = state
        .get_worker(task_id)
        .ok_or_else(|| anyhow::anyhow!("Task {} not found in parallel state", task_id))?;

    // Check if retryable
    match worker.lifecycle {
        WorkerLifecycle::BlockedPush | WorkerLifecycle::Failed => {
            // Retryable - reset to Running and let the worker resume
            let mut updated_worker = worker.clone();
            updated_worker.lifecycle = WorkerLifecycle::Running;
            updated_worker.last_error = None;
            // Don't reset push_attempts - keep history

            state.upsert_worker(updated_worker);
            save_state(&state_path, &state).context("Failed to save updated worker state")?;

            println!("Worker {} marked for retry.", task_id);
            println!("Run `ralph run loop --parallel N` to resume parallel execution.");

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
