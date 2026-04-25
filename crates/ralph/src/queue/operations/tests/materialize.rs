//! Tests for shared queue task materialization helpers.
//!
//! Purpose:
//! - Verify the queue-owned task materialization path used by follow-ups, decomposition, and task-build normalization.
//!
//! Responsibilities:
//! - Cover contiguous ID allocation, local dependency remapping, insertion policy, and pre-commit validation failures.
//!
//! Scope:
//! - Limited to `queue/operations/materialize.rs`.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Specs are supplied in the caller's desired creation order.
//! - Validation failures must leave the original queue unchanged.

use super::*;

fn spec(local_key: &str, title: &str) -> MaterializedTaskSpec {
    MaterializedTaskSpec {
        local_key: local_key.to_string(),
        title: title.to_string(),
        description: Some(format!("{title} description")),
        priority: TaskPriority::Medium,
        status: TaskStatus::Todo,
        tags: vec!["queue".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do it".to_string()],
        notes: vec![],
        request: Some("shared request".to_string()),
        relates_to: vec![],
        parent_local_key: None,
        parent_task_id: None,
        depends_on_local_keys: vec![],
        estimated_minutes: None,
    }
}

fn options<'a>(
    insertion: MaterializeInsertion,
    now_rfc3339: &'a str,
) -> MaterializeTaskGraphOptions<'a> {
    MaterializeTaskGraphOptions {
        now_rfc3339,
        id_prefix: "RQ",
        id_width: 4,
        max_dependency_depth: 10,
        insertion,
        dry_run: false,
    }
}

#[test]
fn materialize_specs_allocates_contiguous_ids_across_active_and_done() -> anyhow::Result<()> {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0002")],
    };
    let mut done_task = task("RQ-0005");
    done_task.status = TaskStatus::Done;
    done_task.completed_at = Some("2026-01-19T00:00:00Z".to_string());
    let done = QueueFile {
        version: 1,
        tasks: vec![done_task],
    };
    let specs = vec![spec("alpha", "Alpha"), spec("beta", "Beta")];

    let report = apply_materialized_task_graph(
        &mut active,
        Some(&done),
        &specs,
        &options(
            MaterializeInsertion::QueueDefaultTop,
            "2026-04-25T18:00:00Z",
        ),
    )?;

    assert_eq!(
        report
            .created_tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec!["RQ-0006", "RQ-0007"]
    );
    assert_eq!(active.tasks[0].id, "RQ-0006");
    assert_eq!(active.tasks[1].id, "RQ-0007");
    Ok(())
}

#[test]
fn materialize_specs_remaps_local_dependencies_and_stamps_request_created_at_updated_at()
-> anyhow::Result<()> {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task_with(
            "RQ-0001",
            TaskStatus::Doing,
            vec!["queue".to_string()],
        )],
    };
    let mut alpha = spec("alpha", "Alpha");
    alpha.request = Some("follow shared path".to_string());
    let mut beta = spec("beta", "Beta");
    beta.depends_on_local_keys = vec!["alpha".to_string()];

    let report = apply_materialized_task_graph(
        &mut active,
        None,
        &[alpha, beta],
        &options(
            MaterializeInsertion::QueueDefaultTop,
            "2026-04-25T18:05:00Z",
        ),
    )?;

    assert_eq!(active.tasks[1].id, "RQ-0002");
    assert_eq!(active.tasks[2].depends_on, vec!["RQ-0002".to_string()]);
    assert_eq!(
        report.created_tasks[0].request.as_deref(),
        Some("follow shared path")
    );
    assert_eq!(
        report.created_tasks[0].created_at.as_deref(),
        Some("2026-04-25T18:05:00Z")
    );
    assert_eq!(
        report.created_tasks[0].updated_at.as_deref(),
        Some("2026-04-25T18:05:00Z")
    );
    Ok(())
}

#[test]
fn materialize_specs_rejects_unknown_local_dependency_before_mutation() {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let before = serde_json::to_value(&active).expect("queue snapshot");
    let mut broken = spec("alpha", "Alpha");
    broken.depends_on_local_keys = vec!["missing".to_string()];

    let err = apply_materialized_task_graph(
        &mut active,
        None,
        &[broken],
        &options(
            MaterializeInsertion::QueueDefaultTop,
            "2026-04-25T18:10:00Z",
        ),
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("unknown local dependency key: missing"));
    assert_eq!(
        serde_json::to_value(&active).expect("queue snapshot"),
        before
    );
}

#[test]
fn materialize_specs_rejects_self_dependency_before_mutation() {
    let mut active = QueueFile {
        version: 1,
        tasks: vec![task("RQ-0001")],
    };
    let before = serde_json::to_value(&active).expect("queue snapshot");
    let mut broken = spec("alpha", "Alpha");
    broken.depends_on_local_keys = vec!["alpha".to_string()];

    let err = apply_materialized_task_graph(
        &mut active,
        None,
        &[broken],
        &options(
            MaterializeInsertion::QueueDefaultTop,
            "2026-04-25T18:10:00Z",
        ),
    )
    .unwrap_err();

    assert!(format!("{err:#}").contains("task local_key alpha depends on itself"));
    assert_eq!(
        serde_json::to_value(&active).expect("queue snapshot"),
        before
    );
}

#[test]
fn materialize_specs_replace_subtree_inserts_after_parent_and_before_next_sibling()
-> anyhow::Result<()> {
    let parent = task("RQ-0001");
    let mut old_child = task("RQ-0002");
    old_child.parent_id = Some("RQ-0001".to_string());
    let sibling = task("RQ-0003");
    let mut active = QueueFile {
        version: 1,
        tasks: vec![parent, old_child, sibling],
    };
    let mut replacement = spec("new-child", "New child");
    replacement.parent_task_id = Some("RQ-0001".to_string());

    apply_materialized_task_graph(
        &mut active,
        None,
        &[replacement],
        &options(
            MaterializeInsertion::ReplaceSubtree {
                parent_task_id: "RQ-0001".to_string(),
                removed_subtree_task_ids: vec!["RQ-0002".to_string()],
            },
            "2026-04-25T18:15:00Z",
        ),
    )?;

    assert_eq!(
        active
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec!["RQ-0001", "RQ-0004", "RQ-0003"]
    );
    assert_eq!(active.tasks[1].parent_id.as_deref(), Some("RQ-0001"));
    Ok(())
}

#[test]
fn materialize_specs_append_under_parent_preserves_preorder_parent_child_layout()
-> anyhow::Result<()> {
    let parent = task("RQ-0001");
    let mut existing_child = task("RQ-0002");
    existing_child.parent_id = Some("RQ-0001".to_string());
    let sibling = task("RQ-0003");
    let mut active = QueueFile {
        version: 1,
        tasks: vec![parent, existing_child, sibling],
    };
    let mut root = spec("auth-root", "Auth root");
    root.parent_task_id = Some("RQ-0001".to_string());
    let mut child = spec("auth-ui", "Auth UI");
    child.parent_local_key = Some("auth-root".to_string());

    apply_materialized_task_graph(
        &mut active,
        None,
        &[root, child],
        &options(
            MaterializeInsertion::AppendUnderParent {
                parent_task_id: "RQ-0001".to_string(),
                existing_subtree_task_ids: vec!["RQ-0002".to_string()],
            },
            "2026-04-25T18:20:00Z",
        ),
    )?;

    assert_eq!(
        active
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec!["RQ-0001", "RQ-0002", "RQ-0004", "RQ-0005", "RQ-0003"]
    );
    assert_eq!(active.tasks[2].parent_id.as_deref(), Some("RQ-0001"));
    assert_eq!(active.tasks[3].parent_id.as_deref(), Some("RQ-0004"));
    Ok(())
}
