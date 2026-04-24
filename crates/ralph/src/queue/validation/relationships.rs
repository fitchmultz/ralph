//! Queue relationship validation.
//!
//! Purpose:
//! - Queue relationship validation.
//!
//! Responsibilities:
//! - Validate `blocks`, `relates_to`, and `duplicates` relationships.
//! - Detect hard relationship errors and non-blocking duplicate warnings.
//!
//! Not handled here:
//! - `depends_on` graph analysis.
//! - `parent_id` hierarchy validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `blocks` relationships must remain acyclic.
//! - Relationship targets must exist in either active or done queues.

use super::{DependencyValidationResult, ValidationWarning, shared::TaskCatalog};
use crate::contracts::{Task, TaskStatus};
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};

pub(crate) fn validate_relationships(
    catalog: &TaskCatalog<'_>,
    result: &mut DependencyValidationResult,
) -> Result<()> {
    let mut blocks_graph: HashMap<&str, Vec<&str>> = HashMap::new();

    for task in &catalog.tasks {
        validate_blocks(task, &catalog.all_task_ids, &mut blocks_graph)?;
        validate_relates_to(task, &catalog.all_task_ids)?;
        validate_duplicates(task, &catalog.all_task_ids, &catalog.all_tasks, result)?;
    }

    ensure_blocks_acyclic(&blocks_graph)
}

fn validate_blocks<'a>(
    task: &'a Task,
    all_task_ids: &HashSet<&'a str>,
    blocks_graph: &mut HashMap<&'a str, Vec<&'a str>>,
) -> Result<()> {
    let task_id = task.id.trim();
    for blocked_id in &task.blocks {
        let blocked_id = blocked_id.trim();
        if blocked_id.is_empty() {
            continue;
        }

        if blocked_id == task_id {
            bail!(
                "Self-blocking detected: task {} blocks itself. Remove the self-reference from the blocks field.",
                task_id
            );
        }

        if !all_task_ids.contains(blocked_id) {
            bail!(
                "Invalid blocks relationship: task {} blocks non-existent task {}. Ensure the blocked task ID exists in .ralph/queue.jsonc or .ralph/done.jsonc.",
                task_id,
                blocked_id
            );
        }

        blocks_graph.entry(task_id).or_default().push(blocked_id);
    }

    Ok(())
}

fn validate_relates_to(task: &Task, all_task_ids: &HashSet<&str>) -> Result<()> {
    let task_id = task.id.trim();
    for related_id in &task.relates_to {
        let related_id = related_id.trim();
        if related_id.is_empty() {
            continue;
        }

        if related_id == task_id {
            bail!(
                "Self-reference in relates_to: task {} relates to itself. Remove the self-reference from the relates_to field.",
                task_id
            );
        }

        if !all_task_ids.contains(related_id) {
            bail!(
                "Invalid relates_to relationship: task {} relates to non-existent task {}. Ensure the related task ID exists in .ralph/queue.jsonc or .ralph/done.jsonc.",
                task_id,
                related_id
            );
        }
    }
    Ok(())
}

fn validate_duplicates(
    task: &Task,
    all_task_ids: &HashSet<&str>,
    all_tasks: &HashMap<&str, &Task>,
    result: &mut DependencyValidationResult,
) -> Result<()> {
    let Some(duplicates_id) = task.duplicates.as_deref() else {
        return Ok(());
    };

    let task_id = task.id.trim();
    let duplicates_id = duplicates_id.trim();
    if duplicates_id == task_id {
        bail!(
            "Self-duplication detected: task {} duplicates itself. Remove the self-reference from the duplicates field.",
            task_id
        );
    }

    if !all_task_ids.contains(duplicates_id) {
        bail!(
            "Invalid duplicates relationship: task {} duplicates non-existent task {}. Ensure the duplicated task ID exists in .ralph/queue.jsonc or .ralph/done.jsonc.",
            task_id,
            duplicates_id
        );
    }

    if let Some(duplicate_task) = all_tasks.get(duplicates_id)
        && matches!(
            duplicate_task.status,
            TaskStatus::Done | TaskStatus::Rejected
        )
    {
        result.warnings.push(ValidationWarning {
            task_id: task_id.to_string(),
            message: format!(
                "Task {} duplicates {} which is already {}. Consider if this duplicate is still needed.",
                task_id,
                duplicates_id,
                if duplicate_task.status == TaskStatus::Done {
                    "done"
                } else {
                    "rejected"
                }
            ),
        });
    }

    Ok(())
}

fn ensure_blocks_acyclic(graph: &HashMap<&str, Vec<&str>>) -> Result<()> {
    let mut visited = HashSet::new();
    let mut recursion_stack = HashSet::new();

    for node in graph.keys() {
        if has_cycle(node, graph, &mut visited, &mut recursion_stack) {
            bail!(
                "Circular blocking detected involving task {}. Task blocking relationships must form a DAG (no cycles). Review the blocks fields to break the cycle.",
                node
            );
        }
    }

    Ok(())
}

fn has_cycle(
    node: &str,
    graph: &HashMap<&str, Vec<&str>>,
    visited: &mut HashSet<String>,
    recursion_stack: &mut HashSet<String>,
) -> bool {
    let key = node.to_string();
    visited.insert(key.clone());
    recursion_stack.insert(key.clone());

    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors {
            if !visited.contains(*neighbor) {
                if has_cycle(neighbor, graph, visited, recursion_stack) {
                    return true;
                }
            } else if recursion_stack.contains(*neighbor) {
                return true;
            }
        }
    }

    recursion_stack.remove(&key);
    false
}
