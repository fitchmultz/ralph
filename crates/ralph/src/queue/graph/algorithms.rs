//! Graph algorithms over `DependencyGraph`.
//!
//! Responsibilities:
//! - Topological sort (Kahn's algorithm) with cycle detection.
//! - Critical path discovery (longest dependency chain) for DAGs.
//! - Queries for runnable and blocked tasks based on dependency completion.
//!
//! Not handled here:
//! - Graph construction (see `build`).
//! - Traversal utilities intended for UI exploration (see `traversal`).
//!
//! Invariants/assumptions:
//! - Normal operation is DAG; cycle detection is still supported for robustness.

use super::types::{CriticalPathResult, DependencyGraph};
use crate::contracts::TaskStatus;
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet, VecDeque};

pub fn topological_sort(graph: &DependencyGraph) -> Result<Vec<String>> {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    for task_id in graph.task_ids() {
        in_degree.entry(task_id.clone()).or_insert(0);
    }

    for task_id in graph.task_ids() {
        let Some(node) = graph.get(task_id) else {
            continue;
        };
        for dep_id in &node.dependencies {
            adjacency
                .entry(dep_id.clone())
                .or_default()
                .push(task_id.clone());
            *in_degree.entry(task_id.clone()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|&(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut result = Vec::new();

    while let Some(current) = queue.pop_front() {
        result.push(current.clone());

        if let Some(neighbors) = adjacency.get(&current) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if result.len() != graph.len() {
        bail!("Cycle detected in dependency graph");
    }

    Ok(result)
}

fn longest_path_from(
    task_id: &str,
    graph: &DependencyGraph,
    memo: &mut HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
) -> Vec<String> {
    if let Some(path) = memo.get(task_id) {
        return path.clone();
    }

    if visited.contains(task_id) {
        return vec![task_id.to_string()];
    }
    visited.insert(task_id.to_string());

    let mut longest = vec![task_id.to_string()];

    if let Some(node) = graph.get(task_id) {
        for dep_id in &node.dependencies {
            let dep_path = longest_path_from(dep_id, graph, memo, visited);
            if dep_path.len() + 1 > longest.len() {
                longest = vec![task_id.to_string()];
                longest.extend(dep_path);
            }
        }
    }

    visited.remove(task_id);
    memo.insert(task_id.to_string(), longest.clone());
    longest
}

pub fn find_critical_paths(graph: &DependencyGraph) -> Vec<CriticalPathResult> {
    if graph.is_empty() {
        return Vec::new();
    }

    let mut memo: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_paths: Vec<CriticalPathResult> = Vec::new();
    let mut max_length = 0;

    for root_id in graph.roots() {
        let mut visited = HashSet::new();
        let path = longest_path_from(root_id, graph, &mut memo, &mut visited);

        if path.len() > max_length {
            max_length = path.len();
            all_paths.clear();
        }

        if path.len() == max_length && max_length > 0 {
            let is_blocked = path.iter().any(|id| !graph.is_task_completed(id));
            all_paths.push(CriticalPathResult {
                path,
                length: max_length,
                is_blocked,
            });
        }
    }

    if all_paths.is_empty() && !graph.is_empty() {
        for task_id in graph.task_ids() {
            let mut visited = HashSet::new();
            let path = longest_path_from(task_id, graph, &mut memo, &mut visited);

            if path.len() > max_length {
                max_length = path.len();
                all_paths.clear();
            }

            if path.len() == max_length {
                let is_blocked = path.iter().any(|id| !graph.is_task_completed(id));
                all_paths.push(CriticalPathResult {
                    path,
                    length: max_length,
                    is_blocked,
                });
            }
        }
    }

    all_paths
}

pub fn find_critical_path_from(
    graph: &DependencyGraph,
    start_task_id: &str,
) -> Option<CriticalPathResult> {
    if !graph.contains(start_task_id) {
        return None;
    }

    let mut memo: HashMap<String, Vec<String>> = HashMap::new();
    let mut visited = HashSet::new();

    let path = longest_path_from(start_task_id, graph, &mut memo, &mut visited);
    let is_blocked = path.iter().any(|id| !graph.is_task_completed(id));

    Some(CriticalPathResult {
        length: path.len(),
        path,
        is_blocked,
    })
}

pub fn get_runnable_tasks(graph: &DependencyGraph) -> Vec<String> {
    graph
        .values()
        .filter(|n| {
            n.task.status == TaskStatus::Todo
                && n.dependencies
                    .iter()
                    .all(|dep_id| graph.is_task_completed(dep_id))
        })
        .map(|n| n.task.id.clone())
        .collect()
}

pub fn get_blocked_tasks(graph: &DependencyGraph) -> Vec<String> {
    graph
        .values()
        .filter(|n| {
            matches!(n.task.status, TaskStatus::Todo | TaskStatus::Doing)
                && n.dependencies
                    .iter()
                    .any(|dep_id| !graph.is_task_completed(dep_id))
        })
        .map(|n| n.task.id.clone())
        .collect()
}
