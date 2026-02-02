//! Core types for the queue dependency graph.
//!
//! Responsibilities:
//! - Define the graph data model (`TaskNode`, `DependencyGraph`) and public result types.
//! - Provide minimal "core" `DependencyGraph` methods that do not belong to algorithms/traversal.
//!
//! Not handled here:
//! - Building graphs from queue files (see `build`).
//! - Graph algorithms (see `algorithms`).
//! - Chain traversal utilities (see `traversal`).
//!
//! Invariants/assumptions:
//! - `DependencyGraph` keys are canonicalized task IDs (trimmed).
//! - Missing task IDs queried via `is_task_completed` are treated as completed (non-blockers).

use crate::contracts::{Task, TaskStatus};
use std::collections::HashMap;

/// A node in the dependency graph representing a task and its relationships.
#[derive(Debug, Clone)]
pub struct TaskNode {
    /// The task data.
    pub task: Task,
    /// IDs of tasks this task depends on (upstream dependencies).
    pub dependencies: Vec<String>,
    /// IDs of tasks that depend on this task (downstream dependents).
    pub dependents: Vec<String>,
    /// IDs of tasks this task blocks (must complete before blocked tasks can run).
    pub blocks: Vec<String>,
    /// IDs of tasks that block this task (reverse of blocks).
    pub blocked_by: Vec<String>,
    /// IDs of tasks this task relates to (loose coupling).
    pub relates_to: Vec<String>,
    /// IDs of tasks that relate to this task (reverse of relates_to).
    pub related_by: Vec<String>,
    /// Task ID that this task duplicates (if any).
    pub duplicates: Option<String>,
    /// IDs of tasks that duplicate this task.
    pub duplicated_by: Vec<String>,
}

/// Dependency graph containing all tasks and their relationships.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub(crate) nodes: HashMap<String, TaskNode>,
    pub(crate) roots: Vec<String>,
    pub(crate) leaves: Vec<String>,
}

/// Result of critical path analysis.
#[derive(Debug, Clone)]
pub struct CriticalPathResult {
    /// Task IDs in the critical path (from root to leaf, following `dependencies`).
    pub path: Vec<String>,
    /// Number of tasks in the path.
    pub length: usize,
    /// Whether any task in the path is blocking (not done/rejected).
    pub is_blocked: bool,
}

/// Output format for graph serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// ASCII art tree structure.
    Tree,
    /// Graphviz DOT format.
    Dot,
    /// JSON format.
    Json,
    /// Flat list with indentation.
    List,
}

/// Result of a bounded chain traversal.
#[derive(Debug, Clone)]
pub struct BoundedChainResult {
    /// Task IDs collected during traversal (at most `limit` items).
    pub task_ids: Vec<String>,
    /// True if there were more tasks available beyond the limit.
    pub truncated: bool,
}

impl BoundedChainResult {
    /// Create a bounded result from a full chain, truncating to `limit`.
    pub fn from_full_chain(chain: Vec<String>, limit: usize) -> Self {
        if limit == 0 {
            return Self {
                task_ids: Vec::new(),
                truncated: !chain.is_empty(),
            };
        }

        if chain.len() <= limit {
            Self {
                task_ids: chain,
                truncated: false,
            }
        } else {
            Self {
                task_ids: chain.into_iter().take(limit).collect(),
                truncated: true,
            }
        }
    }
}

impl DependencyGraph {
    pub(crate) fn from_parts(
        nodes: HashMap<String, TaskNode>,
        roots: Vec<String>,
        leaves: Vec<String>,
    ) -> Self {
        Self {
            nodes,
            roots,
            leaves,
        }
    }

    /// Get a node by task ID.
    pub fn get(&self, task_id: &str) -> Option<&TaskNode> {
        self.nodes.get(task_id)
    }

    /// Check if the graph contains a task.
    pub fn contains(&self, task_id: &str) -> bool {
        self.nodes.contains_key(task_id)
    }

    /// Get all task IDs in the graph.
    pub fn task_ids(&self) -> impl Iterator<Item = &String> {
        self.nodes.keys()
    }

    /// Get root node IDs (tasks with no dependents).
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    /// Get leaf node IDs (tasks with no dependencies).
    pub fn leaves(&self) -> &[String] {
        &self.leaves
    }

    /// Get the number of tasks in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns true if the task is on any critical path.
    pub fn is_on_critical_path(
        &self,
        task_id: &str,
        critical_paths: &[CriticalPathResult],
    ) -> bool {
        critical_paths
            .iter()
            .any(|cp| cp.path.iter().any(|id| id == task_id))
    }

    /// Check if a task is completed (done or rejected). Missing tasks are treated as completed.
    pub fn is_task_completed(&self, task_id: &str) -> bool {
        self.get(task_id)
            .map(|n| matches!(n.task.status, TaskStatus::Done | TaskStatus::Rejected))
            .unwrap_or(true)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &TaskNode> {
        self.nodes.values()
    }
}
