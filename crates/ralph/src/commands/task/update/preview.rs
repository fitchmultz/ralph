//! Dry-run preview helpers for task updates.
//!
//! Purpose:
//! - Dry-run preview helpers for task updates.
//!
//! Responsibilities:
//! - Load the current task state for preview mode.
//! - Render the runner prompt that would be used for a task update.
//! - Print a concise preview without mutating queue or done files.
//!
//! Not handled here:
//! - Queue locking, backup creation, or restore-on-failure behavior.
//! - Actual runner execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Preview mode must not modify repo state.
//! - Prompt rendering uses the same settings pipeline as real updates.

use super::super::TaskUpdateSettings;
use super::runner::build_task_update_prompt;
use super::state::find_task;
use crate::{config, queue};
use anyhow::{Context, Result, anyhow};

pub(super) fn preview_task_update(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<()> {
    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    let task_id = task_id.trim();
    let task = find_task(&before, task_id)
        .ok_or_else(|| anyhow!(crate::error_messages::task_not_found(task_id)))?;

    let prompt = build_task_update_prompt(resolved, task_id, settings)?;

    println!("Dry run - would update task {}:", task_id);
    println!("  Current title: {}", task.title);
    println!("\n  Prompt preview (first 800 chars):");
    let preview_len = prompt.len().min(800);
    println!("{}", &prompt[..preview_len]);
    if prompt.len() > 800 {
        println!("\n  ... ({} more characters)", prompt.len() - 800);
    }
    println!("\n  Note: Actual changes depend on runner analysis of repository state.");
    Ok(())
}
