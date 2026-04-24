//! Queue-file level validation.
//!
//! Purpose:
//! - Queue-file level validation.
//!
//! Responsibilities:
//! - Validate standalone queue file invariants and done-file terminal status rules.
//! - Enforce active/done duplicate-ID rules before graph validation runs.
//!
//! Not handled here:
//! - Dependency, relationship, or parent graph logic.
//! - Queue loading or repair.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Rejected tasks are excluded from duplicate-ID conflicts.
//! - Queue schema version remains pinned to `1`.

use super::task_fields::{
    validate_task_agent_fields, validate_task_id, validate_task_required_fields,
};
use crate::contracts::{QueueFile, TaskStatus};
use anyhow::{Result, bail};
use std::collections::HashSet;

pub(crate) fn validate_queue(queue: &QueueFile, id_prefix: &str, id_width: usize) -> Result<()> {
    if queue.version != 1 {
        bail!(
            "Unsupported queue.jsonc version: {}. Ralph requires version 1. Update the 'version' field in .ralph/queue.jsonc.",
            queue.version
        );
    }
    if id_width == 0 {
        bail!(
            "Invalid id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.jsonc or via --id-width."
        );
    }

    let expected_prefix = super::super::normalize_prefix(id_prefix);
    if expected_prefix.is_empty() {
        bail!(
            "Empty id_prefix: prefix is required. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.jsonc or via --id-prefix."
        );
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for (idx, task) in queue.tasks.iter().enumerate() {
        validate_task_required_fields(idx, task)?;
        validate_task_agent_fields(idx, task)?;
        validate_task_id(idx, &task.id, &expected_prefix, id_width)?;

        if task.status == TaskStatus::Rejected {
            continue;
        }

        let key = task.id.trim();
        if !seen.insert(key) {
            bail!(
                "Duplicate task ID detected: {}. Ensure each task in .ralph/queue.jsonc has a unique ID.",
                key
            );
        }
    }

    Ok(())
}

pub(crate) fn validate_done_queue(
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    let Some(done) = done else {
        return Ok(());
    };

    validate_queue(done, id_prefix, id_width)?;
    validate_done_terminal_status(done)
}

pub(crate) fn validate_cross_file_duplicates(
    active: &QueueFile,
    done: Option<&QueueFile>,
) -> Result<()> {
    let Some(done) = done else {
        return Ok(());
    };

    let active_ids: HashSet<&str> = active
        .tasks
        .iter()
        .filter(|task| task.status != TaskStatus::Rejected)
        .map(|task| task.id.trim())
        .collect();

    for task in &done.tasks {
        if task.status == TaskStatus::Rejected {
            continue;
        }
        let id = task.id.trim();
        if active_ids.contains(id) {
            bail!(
                "Duplicate task ID detected across queue and done: {}. Ensure task IDs are unique across .ralph/queue.jsonc and .ralph/done.jsonc.",
                id
            );
        }
    }

    Ok(())
}

fn validate_done_terminal_status(done: &QueueFile) -> Result<()> {
    for task in &done.tasks {
        if !matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            bail!(
                "Invalid done.jsonc status: task {} has status '{:?}'. .ralph/done.jsonc must contain only done/rejected tasks. Move the task back to .ralph/queue.jsonc or update its status before archiving.",
                task.id,
                task.status
            );
        }
    }

    Ok(())
}
