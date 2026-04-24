//! Queue import normalization helpers.
//!
//! Purpose:
//! - Queue import normalization helpers.
//!
//! Responsibilities:
//! - Trim imported task fields into canonical queue shapes.
//! - Backfill required timestamps for imported tasks.
//! - Normalize list and custom field collections before validation.
//!
//! Not handled here:
//! - Parsing raw import payloads.
//! - Duplicate handling or queue mutation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Terminal tasks must have `completed_at`.
//! - Empty list items and blank custom field keys are discarded before validation.

use std::collections::HashMap;

use crate::contracts::{Task, TaskStatus};

pub(super) fn normalize_task(task: &mut Task, now: &str) {
    task.id = task.id.trim().to_string();
    task.title = task.title.trim().to_string();
    task.tags = normalize_list(&task.tags);
    task.scope = normalize_list(&task.scope);
    task.evidence = normalize_list(&task.evidence);
    task.plan = normalize_list(&task.plan);
    task.notes = normalize_list(&task.notes);
    task.depends_on = normalize_list(&task.depends_on);
    task.blocks = normalize_list(&task.blocks);
    task.relates_to = normalize_list(&task.relates_to);

    let mut normalized_fields = HashMap::new();
    for (key, value) in &task.custom_fields {
        let key = key.trim();
        if !key.is_empty() {
            normalized_fields.insert(key.to_string(), value.trim().to_string());
        }
    }
    task.custom_fields = normalized_fields;

    if task
        .created_at
        .as_ref()
        .is_none_or(|value| value.trim().is_empty())
    {
        task.created_at = Some(now.to_string());
    }
    if task
        .updated_at
        .as_ref()
        .is_none_or(|value| value.trim().is_empty())
    {
        task.updated_at = Some(now.to_string());
    }
    if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected)
        && task
            .completed_at
            .as_ref()
            .is_none_or(|value| value.trim().is_empty())
    {
        task.completed_at = Some(now.to_string());
    }
}

pub(super) fn normalize_list(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}
