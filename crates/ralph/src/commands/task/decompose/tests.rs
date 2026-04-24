//! Task decomposition tests.
//!
//! Purpose:
//! - Task decomposition tests.
//!
//! Responsibilities:
//! - Cover planner normalization, attach writes, and replace safety behavior.
//! - Exercise decomposition-specific edge cases without invoking external runners.
//!
//! Not handled here:
//! - End-to-end runner execution.
//! - CLI formatting assertions (covered by CLI tests).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Test queues use local temp directories and repo-scoped `.ralph/*.jsonc` files.
//! - Preview/write flows remain deterministic for the same planned tree.

use super::tree::normalize_response;
use super::types::{
    DecompositionAttachTarget, DecompositionChildPolicy, DecompositionPlan, DecompositionPreview,
    DecompositionSource, DependencyEdgePreview, PlannedNode, RawDecompositionResponse,
    RawPlannedNode, SourceKind, TaskDecomposeOptions,
};
use super::write_task_decomposition;
use crate::config;
use crate::contracts::{Config, QueueFile, Task, TaskStatus};
use crate::queue;
use anyhow::Result;
use tempfile::TempDir;

#[test]
fn normalize_response_resolves_sibling_dependencies() -> Result<()> {
    let raw = RawDecompositionResponse {
        warnings: vec![],
        tree: RawPlannedNode {
            key: Some("root".to_string()),
            title: "Ship OAuth".to_string(),
            description: None,
            plan: vec![],
            tags: vec![],
            scope: vec![],
            depends_on: vec![],
            children: vec![
                RawPlannedNode {
                    key: Some("schema".to_string()),
                    title: "Update schema".to_string(),
                    description: None,
                    plan: vec![],
                    tags: vec![],
                    scope: vec![],
                    depends_on: vec![],
                    children: vec![],
                },
                RawPlannedNode {
                    key: Some("ui".to_string()),
                    title: "Wire the UI".to_string(),
                    description: None,
                    plan: vec![],
                    tags: vec![],
                    scope: vec![],
                    depends_on: vec!["schema".to_string()],
                    children: vec![],
                },
            ],
        },
    };
    let opts = TaskDecomposeOptions {
        source_input: "Ship OAuth".to_string(),
        attach_to_task_id: None,
        max_depth: 3,
        max_children: 5,
        max_nodes: 10,
        status: TaskStatus::Draft,
        child_policy: DecompositionChildPolicy::Fail,
        with_dependencies: true,
        runner_override: None,
        model_override: None,
        reasoning_effort_override: None,
        runner_cli_overrides: crate::contracts::RunnerCliOptionsPatch::default(),
        repoprompt_tool_injection: false,
    };

    let plan = normalize_response(raw, SourceKind::Freeform, &opts, "Ship OAuth")?;
    assert_eq!(plan.root.children.len(), 2);
    assert_eq!(
        plan.root.children[1].depends_on_keys,
        vec!["schema".to_string()]
    );
    assert_eq!(plan.dependency_edges.len(), 1);
    Ok(())
}

#[test]
fn write_task_decomposition_attaches_freeform_subtree_under_existing_parent() -> Result<()> {
    let (_temp, resolved) = test_resolved()?;
    let parent = test_task("RQ-0001", "Epic", None);
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![parent.clone()],
        },
    )?;

    let preview = DecompositionPreview {
        source: DecompositionSource::Freeform {
            request: "Build auth".to_string(),
        },
        attach_target: Some(DecompositionAttachTarget {
            task: Box::new(parent.clone()),
            has_existing_children: false,
        }),
        plan: DecompositionPlan {
            root: planned_node(
                "auth-root",
                "Build auth",
                vec![],
                vec![planned_node("auth-ui", "Wire auth UI", vec![], vec![])],
            ),
            warnings: vec![],
            total_nodes: 2,
            leaf_nodes: 1,
            dependency_edges: vec![],
        },
        write_blockers: vec![],
        child_status: TaskStatus::Draft,
        child_policy: DecompositionChildPolicy::Append,
        with_dependencies: false,
    };

    let result = write_task_decomposition(&resolved, &preview, false)?;
    assert_eq!(result.parent_task_id.as_deref(), Some("RQ-0001"));
    assert_eq!(result.created_ids.len(), 2);

    let queue_file = queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_file.tasks.len(), 3);
    assert_eq!(queue_file.tasks[1].parent_id.as_deref(), Some("RQ-0001"));
    assert_eq!(queue_file.tasks[2].parent_id.as_deref(), Some("RQ-0002"));
    Ok(())
}

#[test]
fn write_task_decomposition_replace_rejects_external_references() -> Result<()> {
    let (_temp, resolved) = test_resolved()?;
    let parent = test_task("RQ-0001", "Epic", None);
    let child = test_task("RQ-0002", "Old child", Some("RQ-0001"));
    let mut external = test_task("RQ-0003", "External", None);
    external.depends_on = vec!["RQ-0002".to_string()];
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![parent.clone(), child, external],
        },
    )?;

    let preview = DecompositionPreview {
        source: DecompositionSource::ExistingTask {
            task: Box::new(parent),
        },
        attach_target: None,
        plan: DecompositionPlan {
            root: planned_node(
                "epic",
                "Epic",
                vec![],
                vec![planned_node("new-child", "New child", vec![], vec![])],
            ),
            warnings: vec![],
            total_nodes: 2,
            leaf_nodes: 1,
            dependency_edges: vec![],
        },
        write_blockers: vec![],
        child_status: TaskStatus::Draft,
        child_policy: DecompositionChildPolicy::Replace,
        with_dependencies: false,
    };

    let err = write_task_decomposition(&resolved, &preview, false).unwrap_err();
    assert!(err.to_string().contains("still reference"));
    Ok(())
}

#[test]
fn write_task_decomposition_materializes_sibling_dependencies() -> Result<()> {
    let (_temp, resolved) = test_resolved()?;
    queue::save_queue(&resolved.queue_path, &QueueFile::default())?;
    let preview = DecompositionPreview {
        source: DecompositionSource::Freeform {
            request: "Ship OAuth".to_string(),
        },
        attach_target: None,
        plan: DecompositionPlan {
            root: planned_node(
                "root",
                "Ship OAuth",
                vec![],
                vec![
                    planned_node("schema", "Schema", vec![], vec![]),
                    planned_node("ui", "UI", vec!["schema".to_string()], vec![]),
                ],
            ),
            warnings: vec![],
            total_nodes: 3,
            leaf_nodes: 2,
            dependency_edges: vec![DependencyEdgePreview {
                task_title: "UI".to_string(),
                depends_on_title: "Schema".to_string(),
            }],
        },
        write_blockers: vec![],
        child_status: TaskStatus::Draft,
        child_policy: DecompositionChildPolicy::Fail,
        with_dependencies: true,
    };

    let result = write_task_decomposition(&resolved, &preview, false)?;
    assert_eq!(result.created_ids.len(), 3);
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    assert_eq!(queue_file.tasks[2].depends_on, vec!["RQ-0002".to_string()]);
    Ok(())
}

fn planned_node(
    key: &str,
    title: &str,
    depends_on_keys: Vec<String>,
    children: Vec<PlannedNode>,
) -> PlannedNode {
    PlannedNode {
        planner_key: key.to_string(),
        title: title.to_string(),
        description: None,
        plan: vec![],
        tags: vec![],
        scope: vec![],
        depends_on_keys,
        children,
        dependency_refs: vec![],
    }
}

fn test_resolved() -> Result<(TempDir, config::Resolved)> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let config = Config::default();
    let resolved = config::Resolved {
        config,
        repo_root: repo_root.clone(),
        queue_path: ralph_dir.join("queue.jsonc"),
        done_path: ralph_dir.join("done.jsonc"),
        id_prefix: "RQ".to_string(),
        id_width: 4,
        global_config_path: None,
        project_config_path: Some(ralph_dir.join("config.jsonc")),
    };
    Ok((temp, resolved))
}

fn test_task(id: &str, title: &str, parent_id: Option<&str>) -> Task {
    Task {
        id: id.to_string(),
        title: title.to_string(),
        status: TaskStatus::Todo,
        parent_id: parent_id.map(|value| value.to_string()),
        created_at: Some("2026-03-06T00:00:00Z".to_string()),
        updated_at: Some("2026-03-06T00:00:00Z".to_string()),
        ..Task::default()
    }
}
