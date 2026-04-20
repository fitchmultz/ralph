//! Purpose: Recursive dependency-traversal helpers for queue repair surfaces.
//!
//! Responsibilities:
//! - Walk active and (optional) done queues to collect all task IDs that depend
//!   on a given root task, transitively.
//!
//! Scope:
//! - Pure graph traversal over `QueueFile` references; no IO and no mutation.
//! - Cycle-safe: each task is visited at most once per call.
//!
//! Usage:
//! - Called by recovery and runtime helpers that need to know which tasks would
//!   be impacted before mutating or removing a root task.
//!
//! Invariants/Assumptions:
//! - The returned list excludes the root task itself.
//! - Order reflects a depth-first walk of `depends_on` references; callers that
//!   need a deterministic set should collect into a `HashSet`.

use crate::contracts::QueueFile;

/// Get all tasks that depend on the given task ID (recursively).
/// Returns a list of task IDs that depend on the root task.
pub fn get_dependents(root_id: &str, active: &QueueFile, done: Option<&QueueFile>) -> Vec<String> {
    let mut dependents = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let root_id = root_id.trim();

    fn collect_dependents(
        task_id: &str,
        active: &QueueFile,
        done: Option<&QueueFile>,
        dependents: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(task_id) {
            return;
        }
        visited.insert(task_id.to_string());

        // Check all tasks in active queue
        for task in &active.tasks {
            let current_id = task.id.trim();
            if task.depends_on.iter().any(|d| d.trim() == task_id) {
                if !dependents.contains(&current_id.to_string()) {
                    dependents.push(current_id.to_string());
                }
                collect_dependents(current_id, active, done, dependents, visited);
            }
        }

        // Check all tasks in done archive
        if let Some(done_file) = done {
            for task in &done_file.tasks {
                let current_id = task.id.trim();
                if task.depends_on.iter().any(|d| d.trim() == task_id) {
                    if !dependents.contains(&current_id.to_string()) {
                        dependents.push(current_id.to_string());
                    }
                    collect_dependents(current_id, active, done, dependents, visited);
                }
            }
        }
    }

    collect_dependents(root_id, active, done, &mut dependents, &mut visited);
    dependents.retain(|id| id != root_id);
    dependents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;

    fn task(id: &str, depends_on: Vec<&str>) -> Task {
        Task {
            id: id.to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["evidence".to_string()],
            plan: vec!["plan".to_string()],
            notes: vec![],
            request: Some("request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            estimated_minutes: None,
            actual_minutes: None,
            depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn get_dependents_traverses_active_and_done_recursively() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task("RQ-0001", vec![]),
                task("RQ-0002", vec!["RQ-0001"]),
                task("RQ-0003", vec!["RQ-0002"]),
            ],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0004", vec!["RQ-0003"])],
        };

        let got = get_dependents("RQ-0001", &active, Some(&done));
        let set: std::collections::HashSet<String> = got.into_iter().collect();

        assert!(set.contains("RQ-0002"));
        assert!(set.contains("RQ-0003"));
        assert!(set.contains("RQ-0004"));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn get_dependents_handles_cycles_without_infinite_recursion() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task("RQ-0001", vec!["RQ-0002"]),
                task("RQ-0002", vec!["RQ-0001"]),
            ],
        };

        let got = get_dependents("RQ-0001", &active, None);
        let set: std::collections::HashSet<String> = got.into_iter().collect();

        assert!(set.contains("RQ-0002"));
        assert_eq!(set.len(), 1);
    }
}
