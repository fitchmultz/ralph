//! Dependency graph analysis for task queues.
//!
//! This module provides graph algorithms for analyzing task dependencies,
//! including topological sorting, critical path detection, and chain traversal.
//!
//! Responsibilities:
//! - Build dependency graphs from queue files
//! - Compute topological orderings of tasks
//! - Find critical paths (longest dependency chains)
//! - Traverse blocking and blocked chains
//!
//! Not handled here:
//! - Rendering or visualization (handled by CLI/TUI)
//! - User interaction
//! - File I/O
//!
//! Invariants/assumptions:
//! - The dependency graph is a DAG (no cycles) - validated by queue::validation
//! - Task IDs are unique across active and done queues
//! - All dependencies referenced exist in the graph

use crate::contracts::{QueueFile, Task, TaskStatus};
use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet, VecDeque};

/// A node in the dependency graph representing a task and its relationships.
#[derive(Debug, Clone)]
pub struct TaskNode {
    /// The task data
    pub task: Task,
    /// IDs of tasks this task depends on (upstream dependencies)
    pub dependencies: Vec<String>,
    /// IDs of tasks that depend on this task (downstream dependents)
    pub dependents: Vec<String>,
}

/// Dependency graph containing all tasks and their relationships.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Map from task ID to node
    nodes: HashMap<String, TaskNode>,
    /// IDs of root nodes (tasks with no dependents - nothing depends on them)
    roots: Vec<String>,
    /// IDs of leaf nodes (tasks with no dependencies)
    leaves: Vec<String>,
}

/// Result of critical path analysis.
#[derive(Debug, Clone)]
pub struct CriticalPathResult {
    /// Task IDs in the critical path (from root to leaf)
    pub path: Vec<String>,
    /// Number of tasks in the path
    pub length: usize,
    /// Whether any task in the path is blocking (not done/rejected)
    pub is_blocked: bool,
}

/// Output format for graph serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// ASCII art tree structure
    Tree,
    /// Graphviz DOT format
    Dot,
    /// JSON format
    Json,
    /// Flat list with indentation
    List,
}

impl DependencyGraph {
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

    /// Returns true if the task is on a critical path.
    pub fn is_on_critical_path(
        &self,
        task_id: &str,
        critical_paths: &[CriticalPathResult],
    ) -> bool {
        critical_paths
            .iter()
            .any(|cp| cp.path.contains(&task_id.to_string()))
    }

    /// Get all tasks that block this task (transitive closure of dependencies).
    pub fn get_blocking_chain(&self, task_id: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![task_id.to_string()];

        while let Some(current_id) = stack.pop() {
            if visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            if let Some(node) = self.get(&current_id) {
                for dep_id in &node.dependencies {
                    if !visited.contains(dep_id) {
                        chain.push(dep_id.clone());
                        stack.push(dep_id.clone());
                    }
                }
            }
        }

        chain
    }

    /// Get all tasks blocked by this task (transitive closure of dependents).
    pub fn get_blocked_chain(&self, task_id: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![task_id.to_string()];

        while let Some(current_id) = stack.pop() {
            if visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            if let Some(node) = self.get(&current_id) {
                for dep_id in &node.dependents {
                    if !visited.contains(dep_id) {
                        chain.push(dep_id.clone());
                        stack.push(dep_id.clone());
                    }
                }
            }
        }

        chain
    }

    /// Get immediate dependencies of a task.
    pub fn get_immediate_dependencies(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.dependencies.clone())
            .unwrap_or_default()
    }

    /// Get immediate dependents of a task.
    pub fn get_immediate_dependents(&self, task_id: &str) -> Vec<String> {
        self.get(task_id)
            .map(|n| n.dependents.clone())
            .unwrap_or_default()
    }

    /// Check if a task is completed (done or rejected).
    pub fn is_task_completed(&self, task_id: &str) -> bool {
        self.get(task_id)
            .map(|n| matches!(n.task.status, TaskStatus::Done | TaskStatus::Rejected))
            .unwrap_or(true) // Treat missing as completed (no blocker)
    }
}

/// Build a dependency graph from active and optional done queues.
pub fn build_graph(active: &QueueFile, done: Option<&QueueFile>) -> DependencyGraph {
    let mut nodes = HashMap::new();
    let mut dependents_map: HashMap<String, Vec<String>> = HashMap::new();

    // First pass: collect all tasks and build dependents map
    let all_tasks = active
        .tasks
        .iter()
        .chain(done.iter().flat_map(|d| d.tasks.iter()));

    for task in all_tasks {
        let task_id = task.id.trim().to_string();

        // Track which tasks depend on this task
        for dep_id in &task.depends_on {
            let dep_id = dep_id.trim().to_string();
            dependents_map
                .entry(dep_id)
                .or_default()
                .push(task_id.clone());
        }
    }

    // Second pass: create nodes with both dependencies and dependents
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

        let dependents = dependents_map.get(&task_id).cloned().unwrap_or_default();

        nodes.insert(
            task_id,
            TaskNode {
                task: task.clone(),
                dependencies,
                dependents,
            },
        );
    }

    // Compute roots (tasks with no dependents) and leaves (tasks with no dependencies)
    let roots: Vec<String> = nodes
        .values()
        .filter(|n| n.dependents.is_empty())
        .map(|n| n.task.id.clone())
        .collect();

    let leaves: Vec<String> = nodes
        .values()
        .filter(|n| n.dependencies.is_empty())
        .map(|n| n.task.id.clone())
        .collect();

    DependencyGraph {
        nodes,
        roots,
        leaves,
    }
}

/// Perform topological sort on the dependency graph.
/// Returns task IDs in dependency order (dependencies before dependents).
/// Uses Kahn's algorithm for efficiency.
pub fn topological_sort(graph: &DependencyGraph) -> Result<Vec<String>> {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    // Initialize in-degrees and adjacency list
    for (task_id, node) in &graph.nodes {
        in_degree.entry(task_id.clone()).or_insert(0);
        for dep_id in &node.dependencies {
            adjacency
                .entry(dep_id.clone())
                .or_default()
                .push(task_id.clone());
            *in_degree.entry(task_id.clone()).or_insert(0) += 1;
        }
    }

    // Start with nodes having in-degree 0
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut result = Vec::new();

    while let Some(current) = queue.pop_front() {
        result.push(current.clone());

        if let Some(neighbors) = adjacency.get(&current) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    // Check for cycles
    if result.len() != graph.len() {
        bail!("Cycle detected in dependency graph");
    }

    Ok(result)
}

/// Find the critical path(s) in the dependency graph.
/// The critical path is the longest dependency chain from a root to a leaf.
/// For DAGs, this uses dynamic programming with memoization.
pub fn find_critical_paths(graph: &DependencyGraph) -> Vec<CriticalPathResult> {
    if graph.is_empty() {
        return Vec::new();
    }

    // Memoization: task_id -> longest path starting from this task
    let mut memo: HashMap<String, Vec<String>> = HashMap::new();

    fn longest_path_from(
        task_id: &str,
        graph: &DependencyGraph,
        memo: &mut HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
    ) -> Vec<String> {
        // Check memo
        if let Some(path) = memo.get(task_id) {
            return path.clone();
        }

        // Cycle guard (shouldn't happen in valid graphs)
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

    // Find longest paths from all roots
    let mut all_paths: Vec<CriticalPathResult> = Vec::new();
    let mut max_length = 0;

    for root_id in &graph.roots {
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

    // If no roots found (shouldn't happen in valid graphs), try all nodes
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

/// Find the critical path starting from a specific task.
/// Returns the longest path from this task to any leaf.
pub fn find_critical_path_from(
    graph: &DependencyGraph,
    start_task_id: &str,
) -> Option<CriticalPathResult> {
    if !graph.contains(start_task_id) {
        return None;
    }

    let mut memo: HashMap<String, Vec<String>> = HashMap::new();
    let mut visited = HashSet::new();

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

    let path = longest_path_from(start_task_id, graph, &mut memo, &mut visited);
    let is_blocked = path.iter().any(|id| !graph.is_task_completed(id));

    Some(CriticalPathResult {
        length: path.len(),
        path,
        is_blocked,
    })
}

/// Get tasks that are ready to run (all dependencies satisfied).
pub fn get_runnable_tasks(graph: &DependencyGraph) -> Vec<String> {
    graph
        .nodes
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

/// Get tasks that are blocked (have incomplete dependencies).
pub fn get_blocked_tasks(graph: &DependencyGraph) -> Vec<String> {
    graph
        .nodes
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn task(id: &str, depends_on: Vec<&str>, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: format!("Task {}", id),
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["test".to_string()],
            evidence: vec!["test".to_string()],
            plan: vec!["test".to_string()],
            notes: vec![],
            request: Some("test".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
            custom_fields: HashMap::new(),
        }
    }

    fn queue_file(tasks: Vec<Task>) -> QueueFile {
        QueueFile { version: 1, tasks }
    }

    #[test]
    fn build_graph_creates_correct_structure() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Todo),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
            task("RQ-0003", vec!["RQ-0001"], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);

        assert_eq!(graph.len(), 3);
        assert!(graph.contains("RQ-0001"));
        assert!(graph.contains("RQ-0002"));
        assert!(graph.contains("RQ-0003"));

        // RQ-0001 should have 2 dependents
        let node1 = graph.get("RQ-0001").unwrap();
        assert_eq!(node1.dependents.len(), 2);
        assert!(node1.dependents.contains(&"RQ-0002".to_string()));
        assert!(node1.dependents.contains(&"RQ-0003".to_string()));

        // RQ-0002 and RQ-0003 should each have 1 dependency
        let node2 = graph.get("RQ-0002").unwrap();
        assert_eq!(node2.dependencies.len(), 1);
        assert_eq!(node2.dependencies[0], "RQ-0001");
    }

    #[test]
    fn topological_sort_orders_dependencies_first() {
        let active = queue_file(vec![
            task("RQ-0003", vec!["RQ-0002"], TaskStatus::Todo),
            task("RQ-0001", vec![], TaskStatus::Todo),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);
        let sorted = topological_sort(&graph).unwrap();

        // RQ-0001 should come before RQ-0002, which should come before RQ-0003
        let idx1 = sorted.iter().position(|id| id == "RQ-0001").unwrap();
        let idx2 = sorted.iter().position(|id| id == "RQ-0002").unwrap();
        let idx3 = sorted.iter().position(|id| id == "RQ-0003").unwrap();

        assert!(idx1 < idx2);
        assert!(idx2 < idx3);
    }

    #[test]
    fn find_critical_paths_finds_longest_chain() {
        // Create a graph:
        // RQ-0001 <- RQ-0002 <- RQ-0003
        //      \
        //       <- RQ-0004
        // Critical path should be: RQ-0001, RQ-0002, RQ-0003 (length 3)
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Done),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
            task("RQ-0003", vec!["RQ-0002"], TaskStatus::Todo),
            task("RQ-0004", vec!["RQ-0001"], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);
        let paths = find_critical_paths(&graph);

        assert!(!paths.is_empty());
        let path = &paths[0];
        assert_eq!(path.length, 3);
        assert_eq!(path.path, vec!["RQ-0003", "RQ-0002", "RQ-0001"]);
    }

    #[test]
    fn get_blocking_chain_returns_all_dependencies() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Done),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
            task("RQ-0003", vec!["RQ-0002"], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);
        let chain = graph.get_blocking_chain("RQ-0003");

        assert_eq!(chain.len(), 2);
        assert!(chain.contains(&"RQ-0001".to_string()));
        assert!(chain.contains(&"RQ-0002".to_string()));
    }

    #[test]
    fn get_blocked_chain_returns_all_dependents() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Done),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
            task("RQ-0003", vec!["RQ-0002"], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);
        let chain = graph.get_blocked_chain("RQ-0001");

        assert_eq!(chain.len(), 2);
        assert!(chain.contains(&"RQ-0002".to_string()));
        assert!(chain.contains(&"RQ-0003".to_string()));
    }

    #[test]
    fn get_runnable_tasks_returns_ready_tasks() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Todo),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
            task("RQ-0003", vec!["RQ-0001"], TaskStatus::Done),
        ]);

        let graph = build_graph(&active, None);
        let runnable = get_runnable_tasks(&graph);

        // RQ-0001 has no dependencies, so it's runnable
        assert!(runnable.contains(&"RQ-0001".to_string()));
        // RQ-0002 depends on RQ-0001 which is not done, so not runnable
        assert!(!runnable.contains(&"RQ-0002".to_string()));
        // RQ-0003 is already done, so not runnable
        assert!(!runnable.contains(&"RQ-0003".to_string()));
    }

    #[test]
    fn get_blocked_tasks_returns_blocked_tasks() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Todo),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);
        let blocked = get_blocked_tasks(&graph);

        // RQ-0002 is blocked because RQ-0001 is not done
        assert!(blocked.contains(&"RQ-0002".to_string()));
        // RQ-0001 has no dependencies, so not blocked
        assert!(!blocked.contains(&"RQ-0001".to_string()));
    }

    #[test]
    fn find_critical_path_from_specific_task() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Done),
            task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
            task("RQ-0003", vec!["RQ-0002"], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);
        let result = find_critical_path_from(&graph, "RQ-0002");

        assert!(result.is_some());
        let path = result.unwrap();
        assert_eq!(path.length, 2);
        assert_eq!(path.path, vec!["RQ-0002", "RQ-0001"]);
    }

    #[test]
    fn build_graph_includes_done_queue() {
        let active = queue_file(vec![task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo)]);
        let done = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Done)]);

        let graph = build_graph(&active, Some(&done));

        assert_eq!(graph.len(), 2);
        assert!(graph.contains("RQ-0001"));
        assert!(graph.contains("RQ-0002"));
    }

    #[test]
    fn is_task_completed_checks_status() {
        let active = queue_file(vec![
            task("RQ-0001", vec![], TaskStatus::Done),
            task("RQ-0002", vec![], TaskStatus::Rejected),
            task("RQ-0003", vec![], TaskStatus::Todo),
        ]);

        let graph = build_graph(&active, None);

        assert!(graph.is_task_completed("RQ-0001"));
        assert!(graph.is_task_completed("RQ-0002"));
        assert!(!graph.is_task_completed("RQ-0003"));
    }
}
