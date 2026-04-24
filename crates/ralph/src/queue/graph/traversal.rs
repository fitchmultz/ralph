//! Chain traversal and relationship queries for `DependencyGraph`.
//!
//! Purpose:
//! - Chain traversal and relationship queries for `DependencyGraph`.
//!
//! Responsibilities:
//! - Provide transitive "chain" traversal helpers over relationship edges.
//! - Provide bounded variants for UI rendering (limit + truncated flag).
//! - Provide immediate relationship accessors.
//!
//! Not handled here:
//! - Graph construction (see `build`).
//! - Topological sort / critical path algorithms (see `algorithms`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Traversals are best-effort and tolerate missing task IDs by returning partial results.
//! - Chains may include duplicates if the graph contains converging paths (preserves prior behavior).

use super::types::{BoundedChainResult, DependencyGraph, TaskNode};
use std::collections::HashSet;

fn collect_chain_unbounded(
    graph: &DependencyGraph,
    start: &str,
    mut neighbors: impl FnMut(&TaskNode) -> Vec<String>,
) -> Vec<String> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = vec![start.to_string()];

    while let Some(current_id) = stack.pop() {
        if visited.contains(&current_id) {
            continue;
        }
        visited.insert(current_id.clone());

        if let Some(node) = graph.get(&current_id) {
            for next_id in neighbors(node) {
                if !visited.contains(&next_id) {
                    chain.push(next_id.clone());
                    stack.push(next_id);
                }
            }
        }
    }

    chain
}

fn collect_chain_bounded(
    graph: &DependencyGraph,
    start: &str,
    limit: usize,
    mut neighbors: impl FnMut(&TaskNode) -> Vec<String>,
) -> BoundedChainResult {
    if limit == 0 {
        let mut visited = HashSet::new();
        let mut stack = vec![start.to_string()];

        while let Some(current_id) = stack.pop() {
            if visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            if let Some(node) = graph.get(&current_id) {
                for next_id in neighbors(node) {
                    if !visited.contains(&next_id) {
                        return BoundedChainResult {
                            task_ids: Vec::new(),
                            truncated: true,
                        };
                    }
                }
            }
        }

        return BoundedChainResult {
            task_ids: Vec::new(),
            truncated: false,
        };
    }

    let mut result = BoundedChainResult {
        task_ids: Vec::new(),
        truncated: false,
    };

    let mut visited = HashSet::new();
    let mut stack = vec![start.to_string()];

    while let Some(current_id) = stack.pop() {
        if visited.contains(&current_id) {
            continue;
        }
        visited.insert(current_id.clone());

        if let Some(node) = graph.get(&current_id) {
            for next_id in neighbors(node) {
                if !visited.contains(&next_id) {
                    if result.task_ids.len() < limit {
                        result.task_ids.push(next_id.clone());
                    } else {
                        result.truncated = true;
                    }
                    stack.push(next_id);
                }
            }
        }
    }

    result
}

impl DependencyGraph {
    pub fn get_blocking_chain(&self, task_id: &str) -> Vec<String> {
        collect_chain_unbounded(self, task_id, |n| n.dependencies.clone())
    }

    pub fn get_blocked_chain(&self, task_id: &str) -> Vec<String> {
        collect_chain_unbounded(self, task_id, |n| n.dependents.clone())
    }

    pub fn get_blocking_chain_bounded(&self, task_id: &str, limit: usize) -> BoundedChainResult {
        collect_chain_bounded(self, task_id, limit, |n| n.dependencies.clone())
    }

    pub fn get_blocked_chain_bounded(&self, task_id: &str, limit: usize) -> BoundedChainResult {
        collect_chain_bounded(self, task_id, limit, |n| n.dependents.clone())
    }

    pub fn get_blocks_chain(&self, task_id: &str) -> Vec<String> {
        collect_chain_unbounded(self, task_id, |n| n.blocks.clone())
    }

    pub fn get_blocked_by_chain(&self, task_id: &str) -> Vec<String> {
        collect_chain_unbounded(self, task_id, |n| n.blocked_by.clone())
    }

    pub fn get_related_chain(&self, task_id: &str) -> Vec<String> {
        collect_chain_unbounded(self, task_id, |n| {
            let mut out = Vec::with_capacity(n.relates_to.len() + n.related_by.len());
            out.extend(n.relates_to.iter().cloned());
            out.extend(n.related_by.iter().cloned());
            out
        })
    }

    pub fn get_duplicate_chain(&self, task_id: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        let mut current = task_id.to_string();

        loop {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current.clone());

            if let Some(node) = self.get(&current)
                && let Some(duplicates) = &node.duplicates
                && !visited.contains(duplicates)
            {
                chain.push(duplicates.clone());
                current = duplicates.clone();
                continue;
            }
            break;
        }

        if let Some(node) = self.get(task_id) {
            for dupe_id in &node.duplicated_by {
                if !visited.contains(dupe_id) {
                    chain.push(dupe_id.clone());
                }
            }
        }

        chain
    }

    pub fn get_immediate_dependencies(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.dependencies.clone())
            .unwrap_or_default()
    }

    pub fn get_immediate_dependents(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.dependents.clone())
            .unwrap_or_default()
    }

    pub fn get_immediate_blocks(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.blocks.clone())
            .unwrap_or_default()
    }

    pub fn get_immediate_blocked_by(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.blocked_by.clone())
            .unwrap_or_default()
    }

    pub fn get_immediate_relates_to(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.relates_to.clone())
            .unwrap_or_default()
    }

    pub fn get_immediate_duplicated_by(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.duplicated_by.clone())
            .unwrap_or_default()
    }
}
