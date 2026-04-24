//! Task updating orchestration for runner-driven task refreshes.
//!
//! Purpose:
//! - Task updating orchestration for runner-driven task refreshes.
//!
//! Responsibilities:
//! - Expose single-task and update-all entrypoints for `ralph task update`.
//! - Coordinate dry-run previews, queue locking, backup creation, runner execution, and result reporting.
//! - Keep the root module as a small facade over focused queue, runner, and reporting helpers.
//!
//! Not handled here:
//! - Prompt rendering and runner invocation details.
//! - Queue backup/restore internals or validation helper implementations.
//! - Field-diff reporting logic or unit-test fixtures.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Dry-run mode must remain side-effect free with respect to queue/done files.
//! - Non-dry-run updates create a backup before invoking the runner.
//! - Queue locks are optional only for the internal no-lock/update-all flows.

mod preview;
mod reporting;
mod runner;
mod state;
#[cfg(test)]
mod tests;

use super::TaskUpdateSettings;
use crate::{config, queue};
use anyhow::{Context, Result, bail};

pub fn update_task(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<()> {
    update_task_impl(resolved, task_id, settings, true)
}

pub fn update_task_without_lock(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<()> {
    update_task_impl(resolved, task_id, settings, false)
}

pub fn update_all_tasks(resolved: &config::Resolved, settings: &TaskUpdateSettings) -> Result<()> {
    let _queue_lock =
        queue::acquire_queue_lock(&resolved.repo_root, "task update", settings.force)?;

    let queue_file = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    if queue_file.tasks.is_empty() {
        bail!("No tasks in queue to update.");
    }

    let task_ids: Vec<String> = queue_file
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect();
    for task_id in task_ids {
        update_task_impl(resolved, &task_id, settings, false)?;
    }

    Ok(())
}

fn update_task_impl(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
    acquire_lock: bool,
) -> Result<()> {
    if settings.dry_run {
        return preview::preview_task_update(resolved, task_id, settings);
    }

    let _queue_lock = if acquire_lock {
        Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "task update",
            settings.force,
        )?)
    } else {
        None
    };

    let backup_path = state::backup_queue_for_update(resolved)?;
    let prepared = state::prepare_task_update(resolved, task_id)?;
    let prompt = runner::build_task_update_prompt(resolved, prepared.task_id.as_str(), settings)?;

    runner::run_task_updater(resolved, settings, &prompt)?;

    let after = state::load_validate_and_save_queue_after_update(
        resolved,
        &backup_path,
        prepared.max_depth,
    )?;
    let done_after = state::load_done_queue(resolved)?;

    if let Some(after_task) = state::find_task(&after, prepared.task_id.as_str()) {
        return reporting::log_task_update_changes(
            &prepared.before_json,
            prepared.task_id.as_str(),
            "updated",
            Some(after_task),
        );
    }

    reporting::log_task_update_changes(
        &prepared.before_json,
        prepared.task_id.as_str(),
        "moved to done.jsonc",
        state::find_task(&done_after, prepared.task_id.as_str()),
    )
}
