//! Queue validation submodule.
//!
//! This module contains the validation entry points (`validate_queue`,
//! `validate_queue_set`) plus internal helpers used to enforce queue invariants:
//! ID formatting/prefix/width, required task fields, RFC3339 timestamps, and
//! dependency correctness (existence + acyclic graph).
//!
//! The parent `queue` module re-exports the public entry points so external
//! callers can continue using `crate::queue::validate_queue` and friends.
//!
//! Responsibilities:
//! - Validate queue file format, task fields, and dependencies.
//! - Detect hard errors (blocking) and warnings (non-blocking) for dependencies.
//!
//! Not handled here:
//! - Queue persistence or modification (see `crate::queue`).
//! - Config file validation (see `crate::config`).
//!
//! Invariants/assumptions:
//! - Task IDs are unique (within and across queue/done files, except for rejected tasks).
//! - Dependencies must form a DAG (directed acyclic graph).
//! - Warnings are collected but do not block queue operations.

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::timeutil;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::{HashMap, HashSet};
use time::UtcOffset;

/// Represents a validation issue that doesn't prevent queue operation but should be reported.
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub task_id: String,
    pub message: String,
}

impl ValidationWarning {
    /// Log this warning using the standard logging framework.
    pub fn log(&self) {
        log::warn!("[{}] {}", self.task_id, self.message);
    }
}

/// Log all validation warnings.
pub fn log_warnings(warnings: &[ValidationWarning]) {
    for warning in warnings {
        warning.log();
    }
}

/// Result of dependency validation containing both errors and warnings.
#[derive(Debug, Default)]
pub struct DependencyValidationResult {
    pub warnings: Vec<ValidationWarning>,
}

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

/// Validates a queue set (active + optional done file).
///
/// Returns a vector of warnings if validation succeeds. Warnings are non-blocking issues
/// that should be reported to the user (e.g., dependencies on rejected tasks).
pub fn validate_queue_set(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<Vec<ValidationWarning>> {
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
    let result = validate_dependencies(active, done, max_dependency_depth)?;

    Ok(result.warnings)
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
        bail!("Missing created_at: task {} (index {}) is missing the 'created_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13.000000000Z').", task.id, index);
    }

    if let Some(ts) = task.updated_at.as_deref() {
        validate_rfc3339("updated_at", index, &task.id, ts)?;
    } else {
        bail!("Missing updated_at: task {} (index {}) is missing the 'updated_at' timestamp. Add a valid RFC3339 timestamp (e.g., '2026-01-19T05:23:13.000000000Z').", task.id, index);
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
            "Missing {}: task {} (index {}) requires a non-empty '{}' field. Add a valid RFC3339 UTC timestamp (e.g., '2026-01-19T05:23:13.000000000Z').",
            label,
            id,
            index,
            label
        );
    }
    let dt = timeutil::parse_rfc3339(trimmed).with_context(|| {
        format!(
            "task[{}] {} must be a valid RFC3339 UTC timestamp (got: {}, id={}). Example: '2026-01-19T05:23:13.000000000Z'.",
            index, label, trimmed, id
        )
    })?;
    if dt.offset() != UtcOffset::UTC {
        bail!(
            "task[{}] {} must be a valid RFC3339 UTC timestamp (got: {}, id={}). Example: '2026-01-19T05:23:13.000000000Z'.",
            index,
            label,
            trimmed,
            id
        );
    }
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

fn validate_dependencies(
    active: &QueueFile,
    done: Option<&QueueFile>,
    max_dependency_depth: u8,
) -> Result<DependencyValidationResult> {
    let mut result = DependencyValidationResult::default();

    // Build a map of all tasks for quick lookup
    let mut all_tasks: HashMap<&str, &Task> = HashMap::new();
    for task in &active.tasks {
        all_tasks.insert(task.id.trim(), task);
    }
    if let Some(done_file) = done {
        for task in &done_file.tasks {
            all_tasks.insert(task.id.trim(), task);
        }
    }

    let all_task_ids: HashSet<&str> = all_tasks.keys().copied().collect();

    // Build adjacency list for cycle detection and depth calculation
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();

    // Collect all tasks to check (both active and done)
    let mut all_tasks_iter: Vec<&Task> = active.tasks.iter().collect();
    if let Some(done_file) = done {
        all_tasks_iter.extend(&done_file.tasks);
    }

    for task in &all_tasks_iter {
        let task_id = task.id.trim();
        for dep_id in &task.depends_on {
            let dep_id = dep_id.trim();
            if dep_id.is_empty() {
                continue;
            }

            // Check for self-reference (hard error)
            if dep_id == task_id {
                bail!(
                    "Self-dependency detected: task {} depends on itself. Remove the self-reference from the depends_on field.",
                    task_id
                );
            }

            // Check that dependency exists (hard error)
            if !all_task_ids.contains(dep_id) {
                bail!(
                    "Invalid dependency: task {} depends on non-existent task {}. Ensure the dependency task ID exists in .ralph/queue.json or .ralph/done.json.",
                    task_id,
                    dep_id
                );
            }

            // Check if dependency is rejected (warning)
            if let Some(dep_task) = all_tasks.get(dep_id) {
                if dep_task.status == TaskStatus::Rejected {
                    result.warnings.push(ValidationWarning {
                        task_id: task_id.to_string(),
                        message: format!(
                            "Task {} depends on rejected task {}. This dependency will never be satisfied.",
                            task_id, dep_id
                        ),
                    });
                }
            }

            // Build graph for cycle detection
            graph.entry(task_id).or_default().push(dep_id);
        }
    }

    // Detect cycles using DFS (hard error)
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

    // Check dependency depth for each task (warning)
    let mut depth_cache: HashMap<String, usize> = HashMap::new();
    for task in &active.tasks {
        let task_id = task.id.trim();
        let depth = calculate_dependency_depth(task_id, &graph, &mut depth_cache);
        if depth > max_dependency_depth as usize {
            result.warnings.push(ValidationWarning {
                task_id: task_id.to_string(),
                message: format!(
                    "Task {} has a dependency chain depth of {}, which exceeds the configured maximum of {}. This may indicate overly complex dependencies.",
                    task_id, depth, max_dependency_depth
                ),
            });
        }
    }

    // Check for blocked dependency chains (warning)
    // A task is "blocked" if all paths from it lead to tasks that will never complete
    // (i.e., all terminal nodes in its dependency tree are not 'done')
    let mut blocked_cache: HashMap<String, bool> = HashMap::new();
    let mut visiting = HashSet::new();

    for task in &active.tasks {
        let task_id = task.id.trim();
        if is_task_blocked(
            task_id,
            &all_tasks,
            &graph,
            &mut visiting,
            &mut blocked_cache,
        ) {
            // Find which dependencies are causing the block
            let blocking_deps =
                find_blocking_dependencies(task_id, &all_tasks, &graph, &blocked_cache);
            if !blocking_deps.is_empty() {
                result.warnings.push(ValidationWarning {
                    task_id: task_id.to_string(),
                    message: format!(
                        "Task {} is blocked: all dependency paths lead to incomplete or rejected tasks. Blocking dependencies: {}.",
                        task_id,
                        blocking_deps.join(", ")
                    ),
                });
            }
        }
    }

    Ok(result)
}

/// Calculate the maximum dependency chain depth for a task.
/// Returns 0 for tasks with no dependencies.
fn calculate_dependency_depth(
    task_id: &str,
    graph: &HashMap<&str, Vec<&str>>,
    cache: &mut HashMap<String, usize>,
) -> usize {
    if let Some(&depth) = cache.get(task_id) {
        return depth;
    }

    let depth = if let Some(deps) = graph.get(task_id) {
        if deps.is_empty() {
            0
        } else {
            1 + deps
                .iter()
                .map(|dep| calculate_dependency_depth(dep, graph, cache))
                .max()
                .unwrap_or(0)
        }
    } else {
        0
    };

    cache.insert(task_id.to_string(), depth);
    depth
}

/// Determine if a task is blocked (all dependency paths lead to incomplete/rejected tasks).
/// A task is NOT blocked if any path leads to a 'done' task.
fn is_task_blocked(
    task_id: &str,
    all_tasks: &HashMap<&str, &Task>,
    graph: &HashMap<&str, Vec<&str>>,
    visiting: &mut HashSet<String>,
    memo: &mut HashMap<String, bool>,
) -> bool {
    // Check memoized result
    if let Some(&blocked) = memo.get(task_id) {
        return blocked;
    }

    // Cycle detection - if we're visiting this node, we're in a cycle
    // Cycles are already caught as errors, so treat as blocked to avoid infinite recursion
    if !visiting.insert(task_id.to_string()) {
        return true;
    }

    let deps = match graph.get(task_id) {
        Some(d) if !d.is_empty() => d,
        _ => {
            // No dependencies - task is blocked if it's not done
            visiting.remove(task_id);
            let is_blocked = match all_tasks.get(task_id) {
                Some(task) => task.status != TaskStatus::Done,
                None => true,
            };
            memo.insert(task_id.to_string(), is_blocked);
            return is_blocked;
        }
    };

    // Task is blocked if ALL dependencies are blocked
    let all_blocked = deps
        .iter()
        .all(|dep_id| is_task_blocked(dep_id, all_tasks, graph, visiting, memo));

    visiting.remove(task_id);
    memo.insert(task_id.to_string(), all_blocked);
    all_blocked
}

/// Find the specific dependencies that are causing a task to be blocked.
fn find_blocking_dependencies(
    task_id: &str,
    all_tasks: &HashMap<&str, &Task>,
    graph: &HashMap<&str, Vec<&str>>,
    blocked_cache: &HashMap<String, bool>,
) -> Vec<String> {
    let mut blocking = Vec::new();

    if let Some(deps) = graph.get(task_id) {
        for dep_id in deps.iter() {
            // A dependency is blocking if:
            // 1. It's marked as blocked in the cache, OR
            // 2. It's a terminal node (no deps) that's not done
            let is_blocking = match blocked_cache.get(*dep_id) {
                Some(true) => true,
                Some(false) => false,
                None => {
                    // Not in cache - check if it's a terminal non-done task
                    let is_terminal = match graph.get(*dep_id) {
                        None => true,
                        Some(deps) => deps.is_empty(),
                    };
                    if is_terminal {
                        match all_tasks.get(*dep_id) {
                            Some(task) => task.status != TaskStatus::Done,
                            None => true,
                        }
                    } else {
                        false
                    }
                }
            };

            if is_blocking {
                blocking.push(dep_id.to_string());
            }
        }
    }

    blocking
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
