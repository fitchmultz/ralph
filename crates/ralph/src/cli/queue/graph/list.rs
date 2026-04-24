//! List-format rendering for `queue graph`.
//!
//! Purpose:
//! - List-format rendering for `queue graph`.
//!
//! Responsibilities:
//! - Render focused and full flat-list graph views.
//! - Group full-graph output by status for quick scanning.
//! - Keep user-facing ordering deterministic.
//!
//! Not handled here:
//! - Graph construction or relationship traversal internals.
//! - Tree, DOT, or JSON rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Status sections render in a fixed order.
//! - Within a status section, task IDs are sorted.

use anyhow::{Result, anyhow};

use crate::contracts::TaskStatus;
use crate::queue::graph::{
    CriticalPathResult, DependencyGraph, get_blocked_tasks, get_runnable_tasks,
};

use super::shared::{critical_suffix, filtered_chain, visible_task_ids};

pub(super) fn render_task_list(
    graph: &DependencyGraph,
    task_id: &str,
    critical_paths: &[CriticalPathResult],
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    let task = graph
        .get(task_id)
        .ok_or_else(|| anyhow!("{}", crate::error_messages::task_not_found(task_id)))?;

    println!("{}: {}", task_id, task.task.title);
    println!("Status: {:?}", task.task.status);

    if graph.is_on_critical_path(task_id, critical_paths) {
        println!("CRITICAL PATH TASK");
    }

    let chain = if reverse {
        println!("\nBlocked tasks (downstream):");
        graph.get_blocked_chain(task_id)
    } else {
        println!("\nDependencies (upstream):");
        graph.get_blocking_chain(task_id)
    };

    for (index, id) in filtered_chain(graph, chain, include_done)
        .iter()
        .enumerate()
    {
        if let Some(node) = graph.get(id) {
            println!(
                "{}{}: {} [{:?}]{}",
                "  ".repeat(index + 1),
                id,
                node.task.title,
                node.task.status,
                critical_suffix(graph, id, critical_paths)
            );
        }
    }

    Ok(())
}

pub(super) fn render_full_list(
    graph: &DependencyGraph,
    critical_paths: &[CriticalPathResult],
    include_done: bool,
) -> Result<()> {
    println!("Task Dependency List\n");

    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);

    println!(
        "Summary: {} total, {} ready, {} blocked\n",
        graph.len(),
        runnable.len(),
        blocked.len()
    );

    let visible_ids = visible_task_ids(graph, include_done);
    for status in [
        TaskStatus::Doing,
        TaskStatus::Todo,
        TaskStatus::Draft,
        TaskStatus::Done,
        TaskStatus::Rejected,
    ] {
        let tasks = visible_ids
            .iter()
            .filter_map(|task_id| {
                let node = graph.get(task_id)?;
                (node.task.status == status).then_some((task_id, node))
            })
            .collect::<Vec<_>>();

        if tasks.is_empty() {
            continue;
        }

        println!("{:?}:", status);
        for (task_id, node) in tasks {
            let mut dependencies = node.dependencies.clone();
            dependencies.sort_unstable();
            let dependency_summary = if dependencies.is_empty() {
                "none".to_string()
            } else {
                dependencies.join(", ")
            };
            println!(
                "  {}: {} (depends on: {}){}",
                task_id,
                node.task.title,
                dependency_summary,
                critical_suffix(graph, task_id, critical_paths)
            );
        }
        println!();
    }

    Ok(())
}
