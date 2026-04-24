//! Shared helpers for `queue graph` renderers.
//!
//! Purpose:
//! - Shared helpers for `queue graph` renderers.
//!
//! Responsibilities:
//! - Centralize common status, filtering, and ordering logic used by all graph formats.
//! - Provide deterministic task ordering for human-readable renderers.
//! - Keep renderer modules focused on output layout instead of queue policy.
//!
//! Not handled here:
//! - Loading queue files or building dependency graphs.
//! - Format-specific output composition.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Visible task filtering treats Done/Rejected as completed.
//! - Returned task ID collections are sorted for deterministic output.

use crate::contracts::TaskStatus;
use crate::queue::graph::{CriticalPathResult, DependencyGraph, TaskNode};

pub(super) fn is_completed_status(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Done | TaskStatus::Rejected)
}

pub(super) fn should_include_node(node: &TaskNode, include_done: bool) -> bool {
    include_done || !is_completed_status(node.task.status)
}

pub(super) fn visible_ids<'a>(
    graph: &DependencyGraph,
    ids: impl IntoIterator<Item = &'a String>,
    include_done: bool,
) -> Vec<String> {
    let mut visible = ids
        .into_iter()
        .filter(|id| {
            graph
                .get(id)
                .is_some_and(|node| should_include_node(node, include_done))
        })
        .cloned()
        .collect::<Vec<_>>();
    visible.sort_unstable();
    visible
}

pub(super) fn visible_roots(graph: &DependencyGraph, include_done: bool) -> Vec<String> {
    visible_ids(graph, graph.roots().iter(), include_done)
}

pub(super) fn visible_task_ids(graph: &DependencyGraph, include_done: bool) -> Vec<String> {
    let mut ids = graph
        .task_ids()
        .filter(|id| {
            graph
                .get(id)
                .is_some_and(|node| should_include_node(node, include_done))
        })
        .cloned()
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

pub(super) fn filtered_chain(
    graph: &DependencyGraph,
    ids: impl IntoIterator<Item = String>,
    include_done: bool,
) -> Vec<String> {
    ids.into_iter()
        .filter(|id| {
            graph
                .get(id)
                .is_some_and(|node| should_include_node(node, include_done))
        })
        .collect()
}

pub(super) fn sort_and_dedup(ids: &mut Vec<String>) {
    ids.sort_unstable();
    ids.dedup();
}

pub(super) fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "todo",
        TaskStatus::Doing => "doing",
        TaskStatus::Done => "done",
        TaskStatus::Rejected => "rejected",
        TaskStatus::Draft => "draft",
    }
}

pub(super) fn status_to_emoji(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "⏳",
        TaskStatus::Doing => "🔄",
        TaskStatus::Done => "✅",
        TaskStatus::Rejected => "❌",
        TaskStatus::Draft => "📝",
    }
}

pub(super) fn escape_label(value: &str) -> String {
    value.replace('"', "\\\"").replace('\n', "\\n")
}

pub(super) fn is_on_critical_path(
    graph: &DependencyGraph,
    task_id: &str,
    critical_paths: &[CriticalPathResult],
) -> bool {
    graph.is_on_critical_path(task_id, critical_paths)
}

pub(super) fn critical_marker(
    graph: &DependencyGraph,
    task_id: &str,
    critical_paths: &[CriticalPathResult],
) -> &'static str {
    if is_on_critical_path(graph, task_id, critical_paths) {
        "* "
    } else {
        "  "
    }
}

pub(super) fn critical_suffix(
    graph: &DependencyGraph,
    task_id: &str,
    critical_paths: &[CriticalPathResult],
) -> &'static str {
    if is_on_critical_path(graph, task_id, critical_paths) {
        " *"
    } else {
        ""
    }
}
