//! Purpose: parallel retry command handler.
//!
//! Responsibilities: reset terminal worker state so the coordinator can re-run it.
//!
//! Scope: retrying blocked or failed parallel workers only.
//!
//! Usage: re-exported through `crate::commands::run::parallel_retry`.
//!
//! Not handled here: status inspection, rendering, or orchestration.
//!
//! Invariants/assumptions: retry only updates workers in terminal failure states and the coordinator re-evaluates them on the next run.

use crate::commands::run::parallel::state::{
    WorkerLifecycle, load_state, save_state, state_file_path,
};
use anyhow::{Context, Result};

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
