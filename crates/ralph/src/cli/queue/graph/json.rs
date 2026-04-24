//! JSON-format rendering for `queue graph`.
//!
//! Purpose:
//! - JSON-format rendering for `queue graph`.
//!
//! Responsibilities:
//! - Render focused and full graph JSON payloads for external tools.
//! - Keep JSON field shapes stable and deterministic.
//! - Reuse shared status/filtering helpers instead of duplicating policy.
//!
//! Not handled here:
//! - CLI argument parsing.
//! - Tree, list, or DOT rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Emitted task collections are sorted for deterministic snapshots.
//! - Critical-path membership is derived from the caller-provided path set.

use anyhow::Result;
use serde_json::json;

use crate::contracts::Task;
use crate::queue::graph::{
    CriticalPathResult, DependencyGraph, get_blocked_tasks, get_runnable_tasks,
};

use super::shared::{is_on_critical_path, sort_and_dedup, status_label, visible_task_ids};

pub(super) fn render_task_json(
    graph: &DependencyGraph,
    task: &Task,
    critical_paths: &[CriticalPathResult],
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    let mut related_ids = if reverse {
        graph.get_blocked_chain(&task.id)
    } else {
        graph.get_blocking_chain(&task.id)
    };
    related_ids.retain(|id| {
        graph.get(id).is_some_and(|node| {
            include_done || !super::shared::is_completed_status(node.task.status)
        })
    });
    sort_and_dedup(&mut related_ids);

    let related = related_ids
        .iter()
        .filter_map(|id| graph.get(id))
        .map(|node| {
            json!({
                "id": node.task.id,
                "title": node.task.title,
                "status": status_label(node.task.status),
                "critical": is_on_critical_path(graph, &node.task.id, critical_paths),
            })
        })
        .collect::<Vec<_>>();

    let output = json!({
        "task": task.id,
        "title": task.title,
        "status": status_label(task.status),
        "critical": is_on_critical_path(graph, &task.id, critical_paths),
        "relationship": if reverse { "blocked_by" } else { "depends_on" },
        "related_tasks": related,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub(super) fn render_full_json(
    graph: &DependencyGraph,
    critical_paths: &[CriticalPathResult],
    include_done: bool,
) -> Result<()> {
    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);

    let tasks = visible_task_ids(graph, include_done)
        .into_iter()
        .filter_map(|id| graph.get(&id))
        .map(|node| {
            let mut dependencies = node.dependencies.clone();
            dependencies.sort_unstable();
            let mut dependents = node.dependents.clone();
            dependents.sort_unstable();

            json!({
                "id": node.task.id,
                "title": node.task.title,
                "status": status_label(node.task.status),
                "dependencies": dependencies,
                "dependents": dependents,
                "critical": is_on_critical_path(graph, &node.task.id, critical_paths),
            })
        })
        .collect::<Vec<_>>();

    let mut critical_paths_json = critical_paths
        .iter()
        .map(|path| {
            json!({
                "path": path.path,
                "length": path.length,
                "blocked": path.is_blocked,
            })
        })
        .collect::<Vec<_>>();
    critical_paths_json
        .sort_by(|left, right| left["path"].to_string().cmp(&right["path"].to_string()));

    let output = json!({
        "summary": {
            "total_tasks": graph.len(),
            "runnable_tasks": runnable.len(),
            "blocked_tasks": blocked.len(),
        },
        "critical_paths": critical_paths_json,
        "tasks": tasks,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
