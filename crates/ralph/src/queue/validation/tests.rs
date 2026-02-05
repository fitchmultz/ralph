//! Unit tests for queue validation.

use super::*;
use crate::contracts::{Task, TaskAgent, TaskStatus};
use std::collections::HashMap;

fn task(id: &str) -> Task {
    task_with(id, TaskStatus::Todo, vec!["code".to_string()])
}

fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags,
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

#[test]
fn validate_rejects_duplicate_ids() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001"), task("RQ-0001")],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.to_lowercase().contains("duplicate"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_allows_missing_request() {
    let mut task = task("RQ-0001");
    task.request = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_allows_empty_lists() {
    let mut task = task("RQ-0001");
    task.tags = vec![];
    task.scope = vec![];
    task.evidence = vec![];
    task.plan = vec![];
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_rejects_missing_created_at() {
    let mut task = task("RQ-0001");
    task.created_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing created_at"));
}

#[test]
fn validate_rejects_missing_updated_at() {
    let mut task = task("RQ-0001");
    task.updated_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing updated_at"));
}

#[test]
fn validate_rejects_invalid_rfc3339() {
    let mut task = task("RQ-0001");
    task.created_at = Some("not a date".to_string());
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
}

#[test]
fn validate_rejects_zero_agent_iterations() {
    let mut task = task("RQ-0001");
    task.agent = Some(TaskAgent {
        runner: None,
        model: None,
        model_effort: crate::contracts::ModelEffort::Default,
        iterations: Some(0),
        followup_reasoning_effort: None,
        runner_cli: None,
    });
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("agent.iterations"));
}

#[test]
fn validate_queue_set_rejects_cross_file_duplicates() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    assert!(format!("{err}").contains("Duplicate task ID detected across queue and done"));
}

#[test]
fn validate_queue_allows_duplicate_if_one_is_rejected() {
    let mut t_rejected = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let queue = QueueFile {
        version: 1,
        tasks: vec![
            task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
            t_rejected,
        ],
    };
    assert!(validate_queue(&queue, "RQ", 4).is_ok());
}

#[test]
fn validate_rejects_done_without_completed_at() {
    let mut task = task("RQ-0001");
    task.status = TaskStatus::Done;
    task.completed_at = None;
    let queue = QueueFile {
        version: 1,
        tasks: vec![task],
    };
    let err = validate_queue(&queue, "RQ", 4).unwrap_err();
    assert!(format!("{err}").contains("Missing completed_at"));
}

#[test]
fn validate_queue_set_allows_duplicate_across_files_if_rejected() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["tag".to_string()],
        )],
    };
    let mut t_rejected = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![t_rejected],
    };
    assert!(validate_queue_set(&active, Some(&done), "RQ", 4, 10).is_ok());

    let mut t_rejected2 = task_with("RQ-0001", TaskStatus::Rejected, vec!["tag".to_string()]);
    t_rejected2.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let active2 = QueueFile {
        version: 1,
        tasks: vec![t_rejected2],
    };
    let mut t_done = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
    t_done.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done2 = QueueFile {
        version: 1,
        tasks: vec![t_done],
    };
    assert!(validate_queue_set(&active2, Some(&done2), "RQ", 4, 10).is_ok());
}

#[test]
fn validate_queue_set_rejects_todo_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["tag".to_string()],
        )],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("done.json") && msg.contains("RQ-0001") && msg.contains("Todo"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_queue_set_rejects_doing_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Doing,
            vec!["tag".to_string()],
        )],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("done.json") && msg.contains("RQ-0001") && msg.contains("Doing"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_queue_set_rejects_draft_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Draft,
            vec!["tag".to_string()],
        )],
    };
    let err = validate_queue_set(&active, Some(&done), "RQ", 4, 10).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("done.json") && msg.contains("RQ-0001") && msg.contains("Draft"),
        "unexpected error: {msg}"
    );
}

#[test]
fn validate_queue_set_allows_terminal_statuses_in_done() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let mut rejected_task = task_with("RQ-0003", TaskStatus::Rejected, vec!["tag".to_string()]);
    rejected_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task, rejected_task],
    };
    assert!(validate_queue_set(&active, Some(&done), "RQ", 4, 10).is_ok());
}

// Tests for dependency edge case validations (RQ-0391)

fn task_with_deps(id: &str, status: TaskStatus, deps: Vec<String>) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: deps,
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

#[test]
fn validate_warns_on_dependency_to_rejected_task() {
    // Task A depends on rejected Task B
    // Should produce warning but not error
    let mut rejected = task_with("RQ-0002", TaskStatus::Rejected, vec![]);
    rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            rejected.clone(),
        ],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![],
    };

    let result = validate_queue_set(&active, Some(&done), "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on rejected dependency");
    let warnings = result.unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.task_id == "RQ-0001" && w.message.contains("rejected")),
        "Should warn about dependency on rejected task"
    );
}

#[test]
fn validate_warns_on_deep_dependency_chain() {
    // Create chain: A -> B -> C -> D -> E -> F -> G -> H -> I -> J -> K -> L (depth 11)
    // With max_depth=10, this should trigger a depth warning
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            task_with_deps("RQ-0003", TaskStatus::Todo, vec!["RQ-0004".to_string()]),
            task_with_deps("RQ-0004", TaskStatus::Todo, vec!["RQ-0005".to_string()]),
            task_with_deps("RQ-0005", TaskStatus::Todo, vec!["RQ-0006".to_string()]),
            task_with_deps("RQ-0006", TaskStatus::Todo, vec!["RQ-0007".to_string()]),
            task_with_deps("RQ-0007", TaskStatus::Todo, vec!["RQ-0008".to_string()]),
            task_with_deps("RQ-0008", TaskStatus::Todo, vec!["RQ-0009".to_string()]),
            task_with_deps("RQ-0009", TaskStatus::Todo, vec!["RQ-0010".to_string()]),
            task_with_deps("RQ-0010", TaskStatus::Todo, vec!["RQ-0011".to_string()]),
            task_with_deps("RQ-0011", TaskStatus::Todo, vec!["RQ-0012".to_string()]),
            task_with_deps("RQ-0012", TaskStatus::Todo, vec![]),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on deep chain");
    let warnings = result.unwrap();
    assert!(
        warnings.iter().any(|w| w.message.contains("depth")),
        "Should warn about deep dependency chain: {:?}",
        warnings
    );
}

#[test]
fn validate_allows_shallow_dependency_chain() {
    // Chain within limit: A -> B -> C (depth 2)
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            task_with_deps("RQ-0003", TaskStatus::Todo, vec![]),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on shallow chain");
    let warnings = result.unwrap();
    assert!(
        !warnings.iter().any(|w| w.message.contains("depth")),
        "Should not warn about shallow dependency chain"
    );
}

#[test]
fn validate_warns_on_blocked_dependency_chain() {
    // A -> B -> C (C is todo with no dependencies - will never complete)
    // C will never complete (not done/rejected), so A and B are blocked
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            task_with_deps("RQ-0003", TaskStatus::Todo, vec![]),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on blocked chain");
    let warnings = result.unwrap();
    assert!(
        warnings.iter().any(|w| w.message.contains("blocked")),
        "Should warn about blocked dependency chain: {:?}",
        warnings
    );
}

#[test]
fn validate_allows_unblocked_chain_with_done_task() {
    // A -> B -> C (C is done)
    // Should be valid, no warning
    let mut done_c = task_with("RQ-0003", TaskStatus::Done, vec![]);
    done_c.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
        ],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_c],
    };

    let result = validate_queue_set(&active, Some(&done), "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on unblocked chain");
    let warnings = result.unwrap();
    assert!(
        !warnings.iter().any(|w| w.message.contains("blocked")),
        "Should not warn about unblocked dependency chain: {:?}",
        warnings
    );
}

#[test]
fn validate_detects_transitive_rejected_dependency() {
    // A -> B -> C (C is rejected)
    // A and B should both warn about blocked paths
    let mut rejected_c = task_with("RQ-0003", TaskStatus::Rejected, vec![]);
    rejected_c.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_deps("RQ-0001", TaskStatus::Todo, vec!["RQ-0002".to_string()]),
            task_with_deps("RQ-0002", TaskStatus::Todo, vec!["RQ-0003".to_string()]),
            rejected_c,
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on rejected dependency");
    let warnings = result.unwrap();

    // Should have warnings for both rejected dependency and blocked chain
    let has_rejected_warning = warnings.iter().any(|w| w.message.contains("rejected"));
    let has_blocked_warning = warnings.iter().any(|w| w.message.contains("blocked"));
    assert!(
        has_rejected_warning || has_blocked_warning,
        "Should warn about rejected or blocked dependency: {:?}",
        warnings
    );
}

#[test]
fn validate_no_warnings_for_valid_dependencies() {
    // Simple valid dependency chain with done task
    let mut done_b = task_with("RQ-0002", TaskStatus::Done, vec![]);
    done_b.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_deps(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["RQ-0002".to_string()],
        )],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_b],
    };

    let result = validate_queue_set(&active, Some(&done), "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on valid dependencies");
    let warnings = result.unwrap();
    assert!(
        warnings.is_empty(),
        "Should have no warnings for valid dependencies: {:?}",
        warnings
    );
}

// Tests for relationship validation (RQ-0438)

fn task_with_relationships(
    id: &str,
    status: TaskStatus,
    blocks: Vec<String>,
    relates_to: Vec<String>,
    duplicates: Option<String>,
) -> Task {
    Task {
        id: id.to_string(),
        status,
        title: "Test task".to_string(),
        priority: Default::default(),
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: None,
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks,
        relates_to,
        duplicates,
        custom_fields: HashMap::new(),
        parent_id: None,
    }
}

#[test]
fn validate_rejects_self_blocking() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["RQ-0001".to_string()], // Self-blocking
            vec![],
            None,
        )],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_err(), "Should error on self-blocking");
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("Self-blocking"),
        "Error should mention self-blocking: {}",
        err
    );
}

#[test]
fn validate_rejects_self_relates_to() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec![],
            vec!["RQ-0001".to_string()], // Self-relates
            None,
        )],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_err(), "Should error on self-relates_to");
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("Self-reference"),
        "Error should mention self-reference: {}",
        err
    );
}

#[test]
fn validate_rejects_self_duplication() {
    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec![],
            vec![],
            Some("RQ-0001".to_string()), // Self-duplicates
        )],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_err(), "Should error on self-duplication");
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("Self-duplication"),
        "Error should mention self-duplication: {}",
        err
    );
}

#[test]
fn validate_rejects_blocks_to_nonexistent_task() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["RQ-9999".to_string()],
                vec![],
                None,
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(
        result.is_err(),
        "Should error on blocks to non-existent task"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("non-existent"),
        "Error should mention non-existent task: {}",
        err
    );
}

#[test]
fn validate_rejects_relates_to_nonexistent_task() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec![],
                vec!["RQ-9999".to_string()],
                None,
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(
        result.is_err(),
        "Should error on relates_to non-existent task"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("non-existent"),
        "Error should mention non-existent task: {}",
        err
    );
}

#[test]
fn validate_rejects_duplicates_nonexistent_task() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec![],
                vec![],
                Some("RQ-9999".to_string()),
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(
        result.is_err(),
        "Should error on duplicates non-existent task"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("non-existent"),
        "Error should mention non-existent task: {}",
        err
    );
}

#[test]
fn validate_rejects_circular_blocking() {
    // RQ-0001 blocks RQ-0002, RQ-0002 blocks RQ-0001 (circular)
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["RQ-0002".to_string()],
                vec![],
                None,
            ),
            task_with_relationships(
                "RQ-0002",
                TaskStatus::Todo,
                vec!["RQ-0001".to_string()],
                vec![],
                None,
            ),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_err(), "Should error on circular blocking");
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("Circular blocking"),
        "Error should mention circular blocking: {}",
        err
    );
}

#[test]
fn validate_warns_on_duplicate_of_done_task() {
    let mut done_task = task_with("RQ-0002", TaskStatus::Done, vec![]);
    done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());

    let active = QueueFile {
        version: 1,
        tasks: vec![task_with_relationships(
            "RQ-0001",
            TaskStatus::Todo,
            vec![],
            vec![],
            Some("RQ-0002".to_string()),
        )],
    };
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };

    let result = validate_queue_set(&active, Some(&done), "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on duplicate of done task");
    let warnings = result.unwrap();
    assert!(
        warnings.iter().any(|w| w.message.contains("done")),
        "Should warn about duplicate of done task: {:?}",
        warnings
    );
}

#[test]
fn validate_allows_valid_relationships() {
    let active = QueueFile {
        version: 1,
        tasks: vec![
            task_with_relationships(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["RQ-0002".to_string()],
                vec!["RQ-0003".to_string()],
                None,
            ),
            task_with_relationships("RQ-0002", TaskStatus::Todo, vec![], vec![], None),
            task_with_relationships("RQ-0003", TaskStatus::Todo, vec![], vec![], None),
        ],
    };

    let result = validate_queue_set(&active, None, "RQ", 4, 10);
    assert!(result.is_ok(), "Should not error on valid relationships");
    let warnings = result.unwrap();
    assert!(
        warnings.is_empty(),
        "Should have no warnings for valid relationships: {:?}",
        warnings
    );
}
