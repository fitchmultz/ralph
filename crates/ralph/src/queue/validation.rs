//! Queue validation submodule.
//!
//! This module contains the validation entry points (`validate_queue`,
//! `validate_queue_set`) plus internal helpers used to enforce queue invariants:
//! ID formatting/prefix/width, required task fields, RFC3339 timestamps, and
//! dependency correctness (existence + acyclic graph).
//!
//! The parent `queue` module re-exports the public entry points so external
//! callers can continue using `crate::queue::validate_queue` and friends.

use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::{HashMap, HashSet};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub fn validate_queue(queue: &QueueFile, id_prefix: &str, id_width: usize) -> Result<()> {
    if queue.version != 1 {
        bail!("Unsupported queue.json version: {}. Ralph requires version 1. Update the 'version' field in .ralph/queue.json.", queue.version);
    }
    if id_width == 0 {
        bail!("Invalid id_width: width must be greater than 0. Set a valid width (e.g., 4) in .ralph/config.json or via --id-width.");
    }

    let expected_prefix = super::normalize_prefix(id_prefix);
    if expected_prefix.is_empty() {
        bail!("Empty id_prefix: prefix is required. Set a non-empty prefix (e.g., 'RQ') in .ralph/config.json or via --id-prefix.");
    }

    let mut seen = HashSet::new();
    for (idx, task) in queue.tasks.iter().enumerate() {
        validate_task_required_fields(idx, task)?;
        validate_task_agent_fields(idx, task)?;
        validate_task_id(idx, &task.id, &expected_prefix, id_width)?;

        if task.status == TaskStatus::Rejected {
            continue;
        }

        let key = task.id.trim().to_string();
        if !seen.insert(key.clone()) {
            bail!("Duplicate task ID detected: {}. Ensure each task in .ralph/queue.json has a unique ID.", key);
        }
    }

    Ok(())
}

fn validate_task_agent_fields(index: usize, task: &Task) -> Result<()> {
    if let Some(agent) = task.agent.as_ref() {
        if let Some(iterations) = agent.iterations {
            if iterations == 0 {
                bail!(
                    "Invalid agent.iterations: task {} (index {}) must specify iterations >= 1.",
                    task.id,
                    index
                );
            }
        }
    }
    Ok(())
}

pub fn validate_queue_set(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    validate_queue(active, id_prefix, id_width)?;
    if let Some(done) = done {
        validate_queue(done, id_prefix, id_width)?;
        validate_done_terminal_status(done)?;

        let active_ids: HashSet<&str> = active
            .tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Rejected)
            .map(|t| t.id.trim())
            .collect();
        for task in &done.tasks {
            if task.status == TaskStatus::Rejected {
                continue;
            }
            let id = task.id.trim();
            if active_ids.contains(id) {
                bail!("Duplicate task ID detected across queue and done: {}. Ensure task IDs are unique across .ralph/queue.json and .ralph/done.json.", id);
            }
        }
    }

    // Validate dependencies
    validate_dependencies(active, done)?;

    Ok(())
}

fn validate_done_terminal_status(done: &QueueFile) -> Result<()> {
    for task in &done.tasks {
        if !matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            bail!(
                "Invalid done.json status: task {} has status '{:?}'. .ralph/done.json must contain only done/rejected tasks. Move the task back to .ralph/queue.json or update its status before archiving.",
                task.id,
                task.status
            );
        }
    }

    Ok(())
}

fn validate_task_required_fields(index: usize, task: &Task) -> Result<()> {
    if task.id.trim().is_empty() {
        bail!("Missing task ID: task at index {} is missing an 'id' field. Add a valid ID (e.g., 'RQ-0001') to the task.", index);
    }
    if task.title.trim().is_empty() {
        bail!("Missing task title: task {} (index {}) is missing a 'title' field. Add a descriptive title (e.g., 'Fix login bug').", task.id, index);
    }
    ensure_list_valid("tags", index, &task.id, &task.tags)?;
    ensure_list_valid("scope", index, &task.id, &task.scope)?;
    ensure_list_valid("evidence", index, &task.id, &task.evidence)?;
    ensure_list_valid("plan", index, &task.id, &task.plan)?;

    // request is optional, so no ensure_field_present check needed.

    // Validate custom field keys
    for (key_idx, (key, _value)) in task.custom_fields.iter().enumerate() {
        let key_trimmed = key.trim();
        if key_trimmed.is_empty() {
            bail!(
                "Empty custom field key: task {} (index {}) has an empty key at custom_fields[{}]. Remove the empty key or provide a valid key.",
                task.id, index, key_idx
            );
        }
        if key_trimmed.chars().any(|c| c.is_whitespace()) {
            bail!(
                "Invalid custom field key: task {} (index {}) has a key with whitespace at custom_fields[{}]: '{}'. Custom field keys must not contain whitespace.",
                task.id, index, key_idx, key_trimmed
            );
        }
    }

    if let Some(ts) = task.created_at.as_deref() {
        validate_rfc3339("created_at", index, &task.id, ts)?;
    } else {
        bail!("Missing created_at: task {} (index {}) is missing the 'created_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13Z').", task.id, index);
    }

    if let Some(ts) = task.updated_at.as_deref() {
        validate_rfc3339("updated_at", index, &task.id, ts)?;
    } else {
        bail!("Missing updated_at: task {} (index {}) is missing the 'updated_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13Z').", task.id, index);
    }

    if let Some(ts) = task.completed_at.as_deref() {
        validate_rfc3339("completed_at", index, &task.id, ts)?;
    } else if task.status == TaskStatus::Done || task.status == TaskStatus::Rejected {
        bail!(
            "Missing completed_at: task {} (index {}) is in status '{:?}' but missing 'completed_at'. Add a valid RFC3339 timestamp.",
            task.id,
            index,
            task.status
        );
    }

    Ok(())
}

fn validate_rfc3339(label: &str, index: usize, id: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!(
            "Missing {}: task {} (index {}) requires a non-empty '{}' field. Add a valid RFC3339 UTC timestamp (e.g., '2026-01-19T05:23:13Z').",
            label,
            id,
            index,
            label
        );
    }
    OffsetDateTime::parse(trimmed, &Rfc3339).with_context(|| {
        format!(
            "task[{}] {} must be a valid RFC3339 UTC timestamp (got: {}, id={}). Example: '2026-01-19T05:23:13Z'.",
            index, label, trimmed, id
        )
    })?;
    Ok(())
}

fn ensure_list_valid(label: &str, index: usize, id: &str, values: &[String]) -> Result<()> {
    for (i, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            bail!(
                "Empty {} item: task {} (index {}) contains an empty string at {}[{}]. Remove the empty item or add content.",
                label,
                id,
                index,
                label,
                i
            );
        }
    }
    Ok(())
}

pub(super) fn validate_task_id(
    index: usize,
    raw_id: &str,
    expected_prefix: &str,
    id_width: usize,
) -> Result<u32> {
    let trimmed = raw_id.trim();
    let (prefix_raw, num_raw) = trimmed.split_once('-').ok_or_else(|| {
        anyhow!(
            "Invalid task ID format: task at index {} has ID '{}' which is missing a '-'. Task IDs must follow the 'PREFIX-NUMBER' format (e.g., '{}-0001').",
            index,
            trimmed,
            expected_prefix
        )
    })?;

    let prefix = prefix_raw.trim().to_uppercase();
    if prefix != expected_prefix {
        bail!(
            "Mismatched task ID prefix: task at index {} has prefix '{}' but expected '{}'. Update the task ID to '{}' or change the prefix in .ralph/config.json.",
            index,
            prefix,
            expected_prefix,
            super::format_id(expected_prefix, 1, id_width)
        );
    }

    let num = num_raw.trim();
    if num.len() != id_width {
        bail!(
            "Invalid task ID width: task at index {} has a numeric suffix of length {} but expected {}. Pad the numeric part with leading zeros (e.g., '{}').",
            index,
            num.len(),
            id_width,
            super::format_id(expected_prefix, num.parse().unwrap_or(1), id_width)
        );
    }
    if !num.chars().all(|c| c.is_ascii_digit()) {
        bail!(
            "Invalid task ID: task at index {} has non-digit characters in its numeric suffix '{}'. Ensure the ID suffix contains only digits (e.g., '0001').",
            index,
            num
        );
    }

    let value: u32 = num.parse().with_context(|| {
        format!(
            "task[{}] id numeric suffix must parse as integer (got: {})",
            index, num
        )
    })?;
    Ok(value)
}

fn validate_dependencies(active: &QueueFile, done: Option<&QueueFile>) -> Result<()> {
    let all_task_ids: HashSet<&str> = active
        .tasks
        .iter()
        .map(|t| t.id.trim())
        .chain(
            done.iter()
                .flat_map(|d| d.tasks.iter().map(|t| t.id.trim())),
        )
        .collect();

    // Build adjacency list for cycle detection
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();

    for task in &active.tasks {
        let task_id = task.id.trim();
        for dep_id in &task.depends_on {
            let dep_id = dep_id.trim();
            if dep_id.is_empty() {
                continue;
            }

            // Check for self-reference
            if dep_id == task_id {
                bail!(
                    "Self-dependency detected: task {} depends on itself. Remove the self-reference from the depends_on field in .ralph/queue.json.",
                    task_id
                );
            }

            // Check that dependency exists
            if !all_task_ids.contains(dep_id) {
                bail!(
                    "Invalid dependency: task {} depends on non-existent task {}. Ensure the dependency task ID exists in .ralph/queue.json or .ralph/done.json.",
                    task_id,
                    dep_id
                );
            }

            // Build graph for cycle detection
            graph.entry(task_id).or_default().push(dep_id);
        }
    }

    // Also check done archive for dependencies
    if let Some(done_file) = done {
        for task in &done_file.tasks {
            let task_id = task.id.trim();
            for dep_id in &task.depends_on {
                let dep_id = dep_id.trim();
                if dep_id.is_empty() {
                    continue;
                }

                // Check for self-reference
                if dep_id == task_id {
                    bail!(
                        "Self-dependency detected: task {} depends on itself. Remove the self-reference from the depends_on field in .ralph/done.json.",
                        task_id
                    );
                }

                // Check that dependency exists
                if !all_task_ids.contains(dep_id) {
                    bail!(
                        "Invalid dependency: task {} depends on non-existent task {}. Ensure the dependency task ID exists in .ralph/queue.json or .ralph/done.json.",
                        task_id,
                        dep_id
                    );
                }

                // Build graph for cycle detection
                graph.entry(task_id).or_default().push(dep_id);
            }
        }
    }

    // Detect cycles using DFS
    let mut visited = std::collections::HashSet::new();
    let mut rec_stack = std::collections::HashSet::new();

    for node in graph.keys() {
        if has_cycle(node, &graph, &mut visited, &mut rec_stack) {
            bail!(
                "Circular dependency detected involving task {}. Task dependencies must form a DAG (no cycles). Review the depends_on fields to break the cycle.",
                node
            );
        }
    }

    Ok(())
}

fn has_cycle(
    node: &str,
    graph: &HashMap<&str, Vec<&str>>,
    visited: &mut std::collections::HashSet<String>,
    rec_stack: &mut std::collections::HashSet<String>,
) -> bool {
    let node_key = node.to_string();
    visited.insert(node_key.clone());
    rec_stack.insert(node_key.clone());

    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors.iter() {
            if !visited.contains(*neighbor) {
                if has_cycle(neighbor, graph, visited, rec_stack) {
                    return true;
                }
            } else if rec_stack.contains(*neighbor) {
                return true;
            }
        }
    }

    rec_stack.remove(&node_key);
    false
}

#[cfg(test)]
mod tests;
