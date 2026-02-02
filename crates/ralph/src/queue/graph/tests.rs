//! Unit tests for `crate::queue::graph`.
//!
//! Responsibilities:
//! - Validate graph construction, traversal, bounded traversal, and algorithms.
//! - Cover both success paths and key failure modes (e.g., cycle detection).
//!
//! Not handled here:
//! - Integration-level CLI/TUI rendering behavior (covered elsewhere).
//!
//! Invariants/assumptions:
//! - Task timestamps are present (to satisfy `Task` invariants in this crate's contracts).

use super::*;
use crate::contracts::{QueueFile, Task, TaskStatus};
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
        scheduled_start: None,
        depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
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

    let node1 = graph.get("RQ-0001").unwrap();
    assert_eq!(node1.dependents.len(), 2);
    assert!(node1.dependents.contains(&"RQ-0002".to_string()));
    assert!(node1.dependents.contains(&"RQ-0003".to_string()));

    let node2 = graph.get("RQ-0002").unwrap();
    assert_eq!(node2.dependencies, vec!["RQ-0001".to_string()]);
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

    let idx1 = sorted.iter().position(|id| id == "RQ-0001").unwrap();
    let idx2 = sorted.iter().position(|id| id == "RQ-0002").unwrap();
    let idx3 = sorted.iter().position(|id| id == "RQ-0003").unwrap();

    assert!(idx1 < idx2);
    assert!(idx2 < idx3);
}

#[test]
fn topological_sort_detects_cycle() {
    let active = queue_file(vec![
        task("RQ-0001", vec!["RQ-0002"], TaskStatus::Todo),
        task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
    ]);

    let graph = build_graph(&active, None);
    let err = topological_sort(&graph).expect_err("expected cycle error");
    assert!(err.to_string().to_lowercase().contains("cycle"));
}

#[test]
fn find_critical_paths_finds_longest_chain() {
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
fn find_critical_path_from_returns_none_for_missing_task() {
    let active = queue_file(vec![task("RQ-0001", vec![], TaskStatus::Todo)]);
    let graph = build_graph(&active, None);
    assert!(find_critical_path_from(&graph, "RQ-9999").is_none());
}

#[test]
fn traversal_covers_non_dependency_edges() {
    let mut a = task("RQ-0001", vec![], TaskStatus::Todo);
    a.blocks = vec!["RQ-0002".to_string()];
    a.relates_to = vec!["RQ-0003".to_string()];
    a.duplicates = Some("RQ-0004".to_string());

    let mut b = task("RQ-0002", vec![], TaskStatus::Todo);
    b.blocks = vec![];

    let c = task("RQ-0003", vec![], TaskStatus::Todo);

    let d = task("RQ-0004", vec![], TaskStatus::Todo);

    let graph = build_graph(&queue_file(vec![a, b, c, d]), None);

    assert!(
        graph
            .get_blocks_chain("RQ-0001")
            .contains(&"RQ-0002".to_string())
    );
    assert!(
        graph
            .get_blocked_by_chain("RQ-0002")
            .contains(&"RQ-0001".to_string())
    );

    let related = graph.get_related_chain("RQ-0001");
    assert!(related.contains(&"RQ-0003".to_string()));

    let dupes = graph.get_duplicate_chain("RQ-0001");
    assert!(dupes.contains(&"RQ-0004".to_string()));

    assert_eq!(
        graph.get_immediate_blocks("RQ-0001"),
        vec!["RQ-0002".to_string()]
    );
}

#[test]
fn get_runnable_and_blocked_tasks_work() {
    let active = queue_file(vec![
        task("RQ-0001", vec![], TaskStatus::Todo),
        task("RQ-0002", vec!["RQ-0001"], TaskStatus::Todo),
    ]);

    let graph = build_graph(&active, None);

    let runnable = get_runnable_tasks(&graph);
    assert!(runnable.contains(&"RQ-0001".to_string()));
    assert!(!runnable.contains(&"RQ-0002".to_string()));

    let blocked = get_blocked_tasks(&graph);
    assert!(blocked.contains(&"RQ-0002".to_string()));
    assert!(!blocked.contains(&"RQ-0001".to_string()));
}

#[test]
fn bounded_chain_from_full_chain_helper_works() {
    let chain = vec!["a".to_string(), "b".to_string(), "c".to_string()];

    let result = BoundedChainResult::from_full_chain(chain.clone(), 5);
    assert_eq!(result.task_ids.len(), 3);
    assert!(!result.truncated);

    let result = BoundedChainResult::from_full_chain(chain.clone(), 2);
    assert_eq!(result.task_ids.len(), 2);
    assert!(result.truncated);

    let result = BoundedChainResult::from_full_chain(chain.clone(), 0);
    assert!(result.task_ids.is_empty());
    assert!(result.truncated);
}
