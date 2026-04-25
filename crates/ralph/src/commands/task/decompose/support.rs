//! Task decomposition support helpers.
//!
//! Purpose:
//! - Task decomposition support helpers.
//!
//! Responsibilities:
//! - Count planned/generated nodes and dependency edges.
//! - Translate decomposition previews into normalized queue materialization specs.
//! - Provide shared normalization and subtree-navigation helpers.
//!
//! Not handled here:
//! - Runner invocation or prompt rendering.
//! - CLI output formatting.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Planner keys are already uniquified before queue materialization.
//! - Replacement safety checks must run before mutating queue state.

use super::types::{
    DecompositionAttachTarget, DecompositionPreview, DecompositionSource, DependencyEdgePreview,
    PlannedNode, SourceKind,
};
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue;
use crate::queue::operations::MaterializedTaskSpec;
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};

pub(super) fn count_nodes(node: &PlannedNode) -> (usize, usize) {
    if node.children.is_empty() {
        return (1, 1);
    }
    node.children.iter().fold((1, 0), |(nodes, leaves), child| {
        let (child_nodes, child_leaves) = count_nodes(child);
        (nodes + child_nodes, leaves + child_leaves)
    })
}

pub(super) fn collect_dependency_edges(node: &PlannedNode) -> Vec<DependencyEdgePreview> {
    let mut edges = Vec::new();
    collect_dependency_edges_inner(node, &mut edges);
    edges
}

fn collect_dependency_edges_inner(node: &PlannedNode, edges: &mut Vec<DependencyEdgePreview>) {
    let dependency_titles = node
        .children
        .iter()
        .map(|child| (child.planner_key.clone(), child.title.clone()))
        .collect::<HashMap<_, _>>();
    for child in &node.children {
        for dependency_key in &child.depends_on_keys {
            if let Some(title) = dependency_titles.get(dependency_key) {
                edges.push(DependencyEdgePreview {
                    task_title: child.title.clone(),
                    depends_on_title: title.clone(),
                });
            }
        }
        collect_dependency_edges_inner(child, edges);
    }
}

pub(super) fn created_node_count(preview: &DecompositionPreview) -> usize {
    if matches!(preview.source, DecompositionSource::ExistingTask { .. })
        && preview.attach_target.is_none()
    {
        preview
            .plan
            .root
            .children
            .iter()
            .map(|child| count_nodes(child).0)
            .sum()
    } else {
        preview.plan.total_nodes
    }
}

pub(super) fn request_context(preview: &DecompositionPreview) -> String {
    match &preview.source {
        DecompositionSource::Freeform { request } => request.clone(),
        DecompositionSource::ExistingTask { task } => task.request.clone().unwrap_or_else(|| {
            if let Some(description) = task.description.as_deref() {
                format!("{}: {}", task.title, description)
            } else {
                task.title.clone()
            }
        }),
    }
}

pub(super) fn done_queue_ref<'a>(
    done: &'a QueueFile,
    done_path: &std::path::Path,
) -> Option<&'a QueueFile> {
    if done.tasks.is_empty() && !done_path.exists() {
        None
    } else {
        Some(done)
    }
}

pub(super) fn kind_for_source(source: &DecompositionSource) -> SourceKind {
    match source {
        DecompositionSource::Freeform { .. } => SourceKind::Freeform,
        DecompositionSource::ExistingTask { .. } => SourceKind::ExistingTask,
    }
}

pub(super) fn looks_like_task_id(candidate: &str, id_prefix: &str, id_width: usize) -> bool {
    let normalized_prefix = queue::normalize_prefix(id_prefix);
    let Some(rest) = candidate
        .trim()
        .strip_prefix(&format!("{normalized_prefix}-"))
    else {
        return false;
    };
    rest.len() >= id_width && rest.chars().all(|ch| ch.is_ascii_digit())
}

pub(super) fn normalize_title(title: &str, fallback: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        fallback.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn normalize_key(value: Option<&str>, fallback_title: &str) -> String {
    let base = value.unwrap_or(fallback_title).trim().to_ascii_lowercase();
    let mut key = String::new();
    let mut last_was_dash = false;
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            key.push('-');
            last_was_dash = true;
        }
    }
    let key = key.trim_matches('-').to_string();
    if key.is_empty() {
        "task".to_string()
    } else {
        key
    }
}

pub(super) fn materialized_specs_for_preview(
    preview: &DecompositionPreview,
    effective_parent: Option<&Task>,
    request: &str,
) -> Vec<MaterializedTaskSpec> {
    let mut specs = Vec::new();
    let mut seen_local_keys = HashSet::new();
    match (&preview.source, effective_parent) {
        (DecompositionSource::ExistingTask { .. }, Some(parent))
            if preview.attach_target.is_none() =>
        {
            collect_materialized_specs_for_nodes(
                &preview.plan.root.children,
                None,
                Some(parent.id.as_str()),
                preview.child_status,
                request,
                &mut seen_local_keys,
                &mut specs,
            );
        }
        (_, parent) => {
            collect_materialized_specs_for_nodes(
                std::slice::from_ref(&preview.plan.root),
                None,
                parent.map(|task| task.id.as_str()),
                preview.child_status,
                request,
                &mut seen_local_keys,
                &mut specs,
            );
        }
    }
    specs
}

pub(super) fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub(super) fn normalize_strings(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || normalized.iter().any(|existing| existing == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}

pub(super) fn push_warning(warnings: &mut Vec<String>, message: &str) {
    if !warnings.iter().any(|existing| existing == message) {
        warnings.push(message.to_string());
    }
}

pub(super) fn descendant_ids_for_parent(
    active: &QueueFile,
    parent_id: &str,
) -> Result<HashSet<String>> {
    let idx = queue::hierarchy::HierarchyIndex::build(active, None);
    if !idx.contains(parent_id) {
        bail!(
            "{}",
            crate::error_messages::source_task_not_found(parent_id, false)
        );
    }
    let mut descendants = HashSet::new();
    collect_descendant_ids(&idx, parent_id, &mut descendants);
    Ok(descendants)
}

pub(super) fn annotate_parent(
    active: &mut QueueFile,
    parent_id: &str,
    source: &DecompositionSource,
    attach_target: Option<&DecompositionAttachTarget>,
    created_tasks: &[Task],
    now: &str,
) -> Result<()> {
    let Some(parent_index) = active.tasks.iter().position(|task| task.id == parent_id) else {
        bail!(
            "{}",
            crate::error_messages::source_task_not_found(parent_id, false)
        );
    };
    let mut updated_parent = active.tasks[parent_index].clone();
    updated_parent.updated_at = Some(now.to_string());
    let note = match (source, attach_target) {
        (DecompositionSource::ExistingTask { task }, None) => format!(
            "Decomposed task {} into {} child tasks on {}.",
            task.id,
            created_tasks.len(),
            now
        ),
        (DecompositionSource::Freeform { request }, Some(_)) => format!(
            "Attached decomposed request '{}' as {} child task(s) on {}.",
            request,
            created_tasks.len(),
            now
        ),
        (DecompositionSource::ExistingTask { task }, Some(_)) => format!(
            "Attached decomposition sourced from {} as {} child task(s) on {}.",
            task.id,
            created_tasks.len(),
            now
        ),
        (DecompositionSource::Freeform { request }, None) => format!(
            "Decomposition write for '{}' created {} task(s) on {}.",
            request,
            created_tasks.len(),
            now
        ),
    };
    updated_parent.notes.push(note);
    active.tasks[parent_index] = updated_parent;
    Ok(())
}

fn collect_descendant_ids(
    idx: &queue::hierarchy::HierarchyIndex<'_>,
    parent_id: &str,
    out: &mut HashSet<String>,
) {
    for child in idx.children_of(parent_id) {
        if out.insert(child.task.id.clone()) {
            collect_descendant_ids(idx, &child.task.id, out);
        }
    }
}

fn collect_materialized_specs_for_nodes(
    nodes: &[PlannedNode],
    parent_local_key: Option<&str>,
    parent_task_id: Option<&str>,
    status: TaskStatus,
    request: &str,
    seen_local_keys: &mut HashSet<String>,
    out: &mut Vec<MaterializedTaskSpec>,
) {
    let mut sibling_local_keys = HashMap::with_capacity(nodes.len());
    for node in nodes {
        let local_key = allocate_materialized_local_key(&node.planner_key, seen_local_keys);
        sibling_local_keys.insert(node.planner_key.clone(), local_key);
    }

    for node in nodes {
        let local_key = sibling_local_keys
            .get(&node.planner_key)
            .expect("assigned sibling local key")
            .clone();
        out.push(MaterializedTaskSpec {
            local_key: local_key.clone(),
            title: node.title.clone(),
            description: node.description.clone(),
            priority: Default::default(),
            status,
            tags: node.tags.clone(),
            scope: node.scope.clone(),
            evidence: Vec::new(),
            plan: node.plan.clone(),
            notes: Vec::new(),
            request: Some(request.to_string()),
            relates_to: Vec::new(),
            parent_local_key: parent_local_key.map(|value| value.to_string()),
            parent_task_id: parent_task_id.map(|value| value.to_string()),
            depends_on_local_keys: node
                .depends_on_keys
                .iter()
                .map(|dependency_key| {
                    sibling_local_keys
                        .get(dependency_key)
                        .cloned()
                        .expect("sibling dependency should resolve during normalization")
                })
                .collect(),
            estimated_minutes: None,
        });

        collect_materialized_specs_for_nodes(
            &node.children,
            Some(local_key.as_str()),
            None,
            status,
            request,
            seen_local_keys,
            out,
        );
    }
}

fn allocate_materialized_local_key(
    planner_key: &str,
    seen_local_keys: &mut HashSet<String>,
) -> String {
    let base = planner_key.to_string();
    let mut candidate = base.clone();
    let mut suffix = 2usize;
    while !seen_local_keys.insert(candidate.clone()) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}
