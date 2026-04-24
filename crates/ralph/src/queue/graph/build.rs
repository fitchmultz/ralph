//! Graph construction from queue files.
//!
//! Purpose:
//! - Graph construction from queue files.
//!
//! Responsibilities:
//! - Build a `DependencyGraph` from the active queue and optional done queue.
//! - Normalize task IDs for graph keys using `trim()`.
//! - Populate forward and reverse relationship edges.
//!
//! Not handled here:
//! - Validating DAG-ness / cycle detection (algorithms can detect; queue validation is elsewhere).
//! - Algorithms (see `algorithms`) and traversal helpers (see `traversal`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task IDs are unique across active+done (enforced elsewhere).
//! - Relationship IDs may contain whitespace and are normalized via `trim()`.

use crate::contracts::QueueFile;
use std::collections::HashMap;

use super::types::{DependencyGraph, TaskNode};

pub fn build_graph(active: &QueueFile, done: Option<&QueueFile>) -> DependencyGraph {
    let mut nodes: HashMap<String, TaskNode> = HashMap::new();

    let mut dependents_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut blocked_by_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut related_by_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut duplicated_by_map: HashMap<String, Vec<String>> = HashMap::new();

    let all_tasks = active
        .tasks
        .iter()
        .chain(done.iter().flat_map(|d| d.tasks.iter()));

    for task in all_tasks {
        let task_id = task.id.trim().to_string();

        for dep_id in &task.depends_on {
            let dep_id = dep_id.trim().to_string();
            if dep_id.is_empty() {
                continue;
            }
            dependents_map
                .entry(dep_id)
                .or_default()
                .push(task_id.clone());
        }

        for blocked_id in &task.blocks {
            let blocked_id = blocked_id.trim().to_string();
            if blocked_id.is_empty() {
                continue;
            }
            blocked_by_map
                .entry(blocked_id)
                .or_default()
                .push(task_id.clone());
        }

        for related_id in &task.relates_to {
            let related_id = related_id.trim().to_string();
            if related_id.is_empty() {
                continue;
            }
            related_by_map
                .entry(related_id)
                .or_default()
                .push(task_id.clone());
        }

        if let Some(duplicates_id) = &task.duplicates {
            let duplicates_id = duplicates_id.trim().to_string();
            if duplicates_id.is_empty() {
                continue;
            }
            duplicated_by_map
                .entry(duplicates_id)
                .or_default()
                .push(task_id.clone());
        }
    }

    let all_tasks = active
        .tasks
        .iter()
        .chain(done.iter().flat_map(|d| d.tasks.iter()));

    for task in all_tasks {
        let task_id = task.id.trim().to_string();

        let dependencies: Vec<String> = task
            .depends_on
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let blocks: Vec<String> = task
            .blocks
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let relates_to: Vec<String> = task
            .relates_to
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let duplicates = task
            .duplicates
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        nodes.insert(
            task_id.clone(),
            TaskNode {
                task: task.clone(),
                dependencies,
                dependents: dependents_map.get(&task_id).cloned().unwrap_or_default(),
                blocks,
                blocked_by: blocked_by_map.get(&task_id).cloned().unwrap_or_default(),
                relates_to,
                related_by: related_by_map.get(&task_id).cloned().unwrap_or_default(),
                duplicates,
                duplicated_by: duplicated_by_map.get(&task_id).cloned().unwrap_or_default(),
            },
        );
    }

    let roots: Vec<String> = nodes
        .iter()
        .filter(|(_, n)| n.dependents.is_empty())
        .map(|(id, _)| id.clone())
        .collect();

    let leaves: Vec<String> = nodes
        .iter()
        .filter(|(_, n)| n.dependencies.is_empty())
        .map(|(id, _)| id.clone())
        .collect();

    DependencyGraph::from_parts(nodes, roots, leaves)
}
