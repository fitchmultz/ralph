//! Change-reporting helpers for task updates.
//!
//! Purpose:
//! - Change-reporting helpers for task updates.
//!
//! Responsibilities:
//! - Compare before/after task payloads and log changed fields.
//! - Report when a task was moved to `done.jsonc` or removed during update.
//!
//! Not handled here:
//! - Queue loading or validation.
//! - Runner execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Input JSON snapshots represent the task state before and after a single update.
//! - Removal from both queue and done is logged as a warning, not treated as a hard error.

use super::super::compare_task_fields;
use crate::contracts::Task;
use anyhow::Result;

pub(super) fn log_task_update_changes(
    before_json: &str,
    task_id: &str,
    location: &str,
    after_task: Option<&Task>,
) -> Result<()> {
    let Some(after_task) = after_task else {
        log::warn!(
            "Task {} was removed during update and not found in done.jsonc.",
            task_id
        );
        return Ok(());
    };

    let after_json = serde_json::to_string(after_task)?;
    if before_json == after_json {
        log::info!("Task {} {}. No changes detected.", task_id, location);
        return Ok(());
    }

    let changed_fields = compare_task_fields(before_json, &after_json)?;
    log::info!(
        "Task {} {}. Changed fields: {}",
        task_id,
        location,
        changed_fields.join(", ")
    );
    Ok(())
}
