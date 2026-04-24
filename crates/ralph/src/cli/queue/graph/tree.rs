//! Tree-format rendering for `queue graph`.
//!
//! Purpose:
//! - Tree-format rendering for `queue graph`.
//!
//! Responsibilities:
//! - Render focused and full dependency trees.
//! - Traverse upstream/downstream relationships for human-readable output.
//! - Keep traversal ordering deterministic for stable CLI output.
//!
//! Not handled here:
//! - Graph construction or critical-path computation.
//! - JSON, DOT, or list rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Relationship traversal only visits tasks present in the graph.
//! - Sibling nodes are emitted in sorted task ID order.

use anyhow::{Result, anyhow};

use crate::queue::graph::{
    CriticalPathResult, DependencyGraph, find_critical_path_from, get_blocked_tasks,
    get_runnable_tasks,
};

use super::shared::{critical_marker, status_to_emoji, visible_ids, visible_roots};

pub(super) fn render_task_tree(
    graph: &DependencyGraph,
    task_id: &str,
    critical_paths: &[CriticalPathResult],
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    let task = graph
        .get(task_id)
        .ok_or_else(|| anyhow!("{}", crate::error_messages::task_not_found(task_id)))?;

    println!("Dependency tree for {}: {}", task_id, task.task.title);

    if reverse {
        println!("\nTasks blocked by this task (downstream):");
        render_dependents_tree(graph, task_id, "", include_done, critical_paths)?;
    } else {
        println!("\nTasks this task depends on (upstream):");
        render_dependencies_tree(graph, task_id, "", include_done, critical_paths, true)?;
    }

    if let Some(cp) = find_critical_path_from(graph, task_id)
        && cp.length > 1
    {
        println!("\nCritical path from this task: {} tasks", cp.length);
        if cp.is_blocked {
            println!("  Status: BLOCKED (incomplete dependencies)");
        } else {
            println!("  Status: Unblocked");
        }
    }

    Ok(())
}

pub(super) fn render_full_tree(
    graph: &DependencyGraph,
    critical_paths: &[CriticalPathResult],
    include_done: bool,
) -> Result<()> {
    println!("Task Dependency Graph\n");

    let runnable = get_runnable_tasks(graph);
    let blocked = get_blocked_tasks(graph);

    println!("Summary:");
    println!("  Total tasks: {}", graph.len());
    println!("  Ready to run: {}", runnable.len());
    println!("  Blocked: {}", blocked.len());

    if let Some(longest_path) = critical_paths.first() {
        println!("  Critical path length: {}", longest_path.length);
    }

    println!("\nDependency Chains:\n");

    let roots = visible_roots(graph, include_done);
    for (index, root_id) in roots.iter().enumerate() {
        render_dependencies_tree(graph, root_id, "", include_done, critical_paths, true)?;
        if index + 1 < roots.len() {
            println!();
        }
    }

    println!("\nLegend:");
    println!("  * = on critical path");
    println!("  ⏳ = todo, 🔄 = doing, ✅ = done, ❌ = rejected");

    Ok(())
}

fn render_dependencies_tree(
    graph: &DependencyGraph,
    task_id: &str,
    prefix: &str,
    include_done: bool,
    critical_paths: &[CriticalPathResult],
    is_last: bool,
) -> Result<()> {
    let node = match graph.get(task_id) {
        Some(node) => node,
        None => return Ok(()),
    };

    if super::shared::is_completed_status(node.task.status) && !include_done {
        return Ok(());
    }

    let branch = if prefix.is_empty() {
        ""
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    };

    println!(
        "{}{}{}{}: {} [{}]",
        prefix,
        branch,
        critical_marker(graph, task_id, critical_paths),
        task_id,
        node.task.title,
        status_to_emoji(node.task.status)
    );

    let dependencies = visible_ids(graph, node.dependencies.iter(), include_done);
    let new_prefix = if prefix.is_empty() {
        (if is_last { "   " } else { "│  " }).to_string()
    } else {
        format!("{}{}", prefix, if is_last { "   " } else { "│  " })
    };

    for (index, dep_id) in dependencies.iter().enumerate() {
        render_dependencies_tree(
            graph,
            dep_id,
            &new_prefix,
            include_done,
            critical_paths,
            index + 1 == dependencies.len(),
        )?;
    }

    Ok(())
}

fn render_dependents_tree(
    graph: &DependencyGraph,
    task_id: &str,
    prefix: &str,
    include_done: bool,
    critical_paths: &[CriticalPathResult],
) -> Result<()> {
    let node = match graph.get(task_id) {
        Some(node) => node,
        None => return Ok(()),
    };

    if super::shared::is_completed_status(node.task.status) && !include_done && !prefix.is_empty() {
        return Ok(());
    }

    if prefix.is_empty() {
        println!(
            "* {}: {} [{}]",
            task_id,
            node.task.title,
            status_to_emoji(node.task.status)
        );
    }

    let dependents = visible_ids(graph, node.dependents.iter(), include_done);
    for (index, dep_id) in dependents.iter().enumerate() {
        let Some(dep_node) = graph.get(dep_id) else {
            continue;
        };

        let is_last = index + 1 == dependents.len();
        println!(
            "{}{}{}{}: {} [{}]",
            prefix,
            if is_last { "└─ " } else { "├─ " },
            critical_marker(graph, dep_id, critical_paths),
            dep_id,
            dep_node.task.title,
            status_to_emoji(dep_node.task.status)
        );

        let new_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });
        render_dependents_tree(graph, dep_id, &new_prefix, include_done, critical_paths)?;
    }

    Ok(())
}
