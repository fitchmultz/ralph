//! DOT-format rendering for `queue graph`.
//!
//! Purpose:
//! - DOT-format rendering for `queue graph`.
//!
//! Responsibilities:
//! - Render Graphviz DOT output for focused or full graphs.
//! - Encode relationship styling for dependency, block, relate, and duplicate edges.
//! - Keep node and edge emission order deterministic.
//!
//! Not handled here:
//! - Graph construction or CLI argument parsing.
//! - Tree, JSON, or list rendering.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Included task IDs are sorted and deduplicated before emission.
//! - Edge emission never references tasks outside the included set.

use anyhow::Result;

use crate::contracts::TaskStatus;
use crate::queue::graph::{DependencyGraph, find_critical_paths};

use super::shared::{escape_label, is_on_critical_path, sort_and_dedup, visible_task_ids};

pub(super) fn render_task_dot(
    graph: &DependencyGraph,
    focus_task: Option<&str>,
    reverse: bool,
    include_done: bool,
) -> Result<()> {
    let critical_paths = find_critical_paths(graph);

    println!("digraph dependencies {{");
    println!("  rankdir=TB;");
    println!("  node [shape=box, style=rounded];");
    println!();

    let included_tasks = included_tasks(graph, focus_task, reverse, include_done);

    for task_id in &included_tasks {
        if let Some(node) = graph.get(task_id) {
            let is_critical = is_on_critical_path(graph, task_id, &critical_paths);
            let color = match node.task.status {
                TaskStatus::Done => "green",
                TaskStatus::Rejected => "gray",
                TaskStatus::Doing => "orange",
                TaskStatus::Draft => "yellow",
                TaskStatus::Todo if is_critical => "red",
                TaskStatus::Todo => "lightblue",
            };
            let style = if is_critical {
                ", style=filled, fillcolor=red, fontcolor=white"
            } else {
                ""
            };
            let label = format!("{}\\n{}", task_id, escape_label(&node.task.title));
            println!(
                "  \"{}\" [label=\"{}\", color={}{}];",
                task_id, label, color, style
            );
        }
    }

    println!();
    render_dependency_edges(graph, &included_tasks, &critical_paths);
    render_blocks_edges(graph, &included_tasks);
    render_related_edges(graph, &included_tasks);
    render_duplicate_edges(graph, &included_tasks);
    println!("}}");

    Ok(())
}

fn included_tasks(
    graph: &DependencyGraph,
    focus_task: Option<&str>,
    reverse: bool,
    include_done: bool,
) -> Vec<String> {
    let mut ids = if let Some(task_id) = focus_task {
        let mut ids = vec![task_id.to_string()];
        let related = if reverse {
            graph.get_blocked_chain(task_id)
        } else {
            graph.get_blocking_chain(task_id)
        };
        ids.extend(super::shared::filtered_chain(graph, related, include_done));
        ids
    } else {
        visible_task_ids(graph, include_done)
    };

    if focus_task.is_some() {
        ids.retain(|id| {
            graph.get(id).is_some_and(|node| {
                include_done || !super::shared::is_completed_status(node.task.status)
            })
        });
    }

    sort_and_dedup(&mut ids);
    ids
}

fn render_dependency_edges(
    graph: &DependencyGraph,
    included_tasks: &[String],
    critical_paths: &[crate::queue::graph::CriticalPathResult],
) {
    for task_id in included_tasks {
        let Some(node) = graph.get(task_id) else {
            continue;
        };

        let mut dependencies = node.dependencies.clone();
        dependencies.sort_unstable();
        for dep_id in dependencies {
            if included_tasks.binary_search(&dep_id).is_ok() {
                let is_critical = is_on_critical_path(graph, task_id, critical_paths)
                    && is_on_critical_path(graph, &dep_id, critical_paths);
                let edge_style = if is_critical {
                    " [color=red, penwidth=2]"
                } else {
                    ""
                };
                println!("  \"{}\" -> \"{}\"{};", task_id, dep_id, edge_style);
            }
        }
    }
}

fn render_blocks_edges(graph: &DependencyGraph, included_tasks: &[String]) {
    for task_id in included_tasks {
        let Some(node) = graph.get(task_id) else {
            continue;
        };

        let mut blocked = node.blocks.clone();
        blocked.sort_unstable();
        for blocked_id in blocked {
            if included_tasks.binary_search(&blocked_id).is_ok() {
                println!(
                    "  \"{}\" -> \"{}\" [style=dashed, color=orange, label=\"blocks\"];",
                    task_id, blocked_id
                );
            }
        }
    }
}

fn render_related_edges(graph: &DependencyGraph, included_tasks: &[String]) {
    for task_id in included_tasks {
        let Some(node) = graph.get(task_id) else {
            continue;
        };

        let mut related = node.relates_to.clone();
        related.sort_unstable();
        for related_id in related {
            if task_id < &related_id && included_tasks.binary_search(&related_id).is_ok() {
                println!(
                    "  \"{}\" -> \"{}\" [style=dotted, color=blue, label=\"relates\"];",
                    task_id, related_id
                );
            }
        }
    }
}

fn render_duplicate_edges(graph: &DependencyGraph, included_tasks: &[String]) {
    for task_id in included_tasks {
        let Some(node) = graph.get(task_id) else {
            continue;
        };

        if let Some(duplicate_id) = &node.duplicates
            && included_tasks.binary_search(duplicate_id).is_ok()
        {
            println!(
                "  \"{}\" -> \"{}\" [style=bold, color=red, label=\"duplicates\"];",
                task_id, duplicate_id
            );
        }
    }
}
