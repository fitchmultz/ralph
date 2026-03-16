//! Task decomposition support helpers.
//!
//! Responsibilities:
//! - Count planned/generated nodes and dependency edges.
//! - Materialize planned nodes into durable queue tasks.
//! - Provide shared normalization, queue insertion, and subtree safety helpers.
//!
//! Not handled here:
//! - Runner invocation or prompt rendering.
//! - CLI output formatting.
//!
//! Invariants/assumptions:
//! - Planner keys are already uniquified among siblings before materialization.
//! - Replacement safety checks must run before mutating queue state.

use super::types::{
    DecompositionAttachTarget, DecompositionChildPolicy, DecompositionPreview, DecompositionSource,
    DependencyEdgePreview, PlannedNode, SourceKind,
};
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue;
use anyhow::{Context, Result, bail};
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

pub(super) fn allocate_sequential_ids(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_depth: u8,
    count: usize,
) -> Result<Vec<String>> {
    let first_id = queue::next_id_across(active, done, id_prefix, id_width, max_depth)?;
    let numeric_start = parse_numeric_id(&first_id, id_prefix)?;
    let normalized_prefix = queue::normalize_prefix(id_prefix);
    let mut ids = Vec::with_capacity(count);
    for offset in 0..count {
        let increment =
            u32::try_from(offset).context("task decomposition generated too many tasks")?;
        ids.push(queue::format_id(
            &normalized_prefix,
            numeric_start + increment,
            id_width,
        ));
    }
    Ok(ids)
}

pub(super) fn materialize_children(
    nodes: &[PlannedNode],
    parent_id: Option<&str>,
    ids: &[String],
    next_id_index: &mut usize,
    status: TaskStatus,
    request: &str,
    now: &str,
) -> Result<Vec<Task>> {
    let mut direct_tasks = Vec::new();
    for node in nodes {
        direct_tasks.push(materialize_node(
            node,
            parent_id,
            ids,
            next_id_index,
            status,
            request,
            now,
        )?);
    }

    let sibling_id_map = nodes
        .iter()
        .zip(direct_tasks.iter())
        .map(|(node, task)| (node.planner_key.clone(), task.id.clone()))
        .collect::<HashMap<_, _>>();

    for (index, task) in direct_tasks.iter_mut().enumerate() {
        task.depends_on = nodes[index]
            .depends_on_keys
            .iter()
            .filter_map(|key| sibling_id_map.get(key).cloned())
            .collect();
    }

    let mut tasks = Vec::new();
    for (task, node) in direct_tasks.into_iter().zip(nodes.iter()) {
        let task_id = task.id.clone();
        tasks.push(task);
        let descendants = materialize_children(
            &node.children,
            Some(task_id.as_str()),
            ids,
            next_id_index,
            status,
            request,
            now,
        )?;
        tasks.extend(descendants);
    }
    Ok(tasks)
}

pub(super) fn materialize_node(
    node: &PlannedNode,
    parent_id: Option<&str>,
    ids: &[String],
    next_id_index: &mut usize,
    status: TaskStatus,
    request: &str,
    now: &str,
) -> Result<Task> {
    let id = ids
        .get(*next_id_index)
        .cloned()
        .context("planner/task count mismatch while assigning IDs")?;
    *next_id_index += 1;
    Ok(Task {
        id,
        status,
        title: node.title.clone(),
        description: node.description.clone(),
        tags: node.tags.clone(),
        scope: node.scope.clone(),
        plan: node.plan.clone(),
        request: Some(request.to_string()),
        created_at: Some(now.to_string()),
        updated_at: Some(now.to_string()),
        parent_id: parent_id.map(|value| value.to_string()),
        ..Task::default()
    })
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

pub(super) fn ensure_subtree_is_replaceable(
    active: &QueueFile,
    done: Option<&QueueFile>,
    removed_ids: &HashSet<String>,
) -> Result<()> {
    let mut references = Vec::new();
    for task in active
        .tasks
        .iter()
        .chain(done.into_iter().flat_map(|queue| queue.tasks.iter()))
    {
        if removed_ids.contains(&task.id) {
            continue;
        }
        for dep in &task.depends_on {
            if removed_ids.contains(dep) {
                references.push(format!("{} depends_on {}", task.id, dep));
            }
        }
        for blocked in &task.blocks {
            if removed_ids.contains(blocked) {
                references.push(format!("{} blocks {}", task.id, blocked));
            }
        }
        for related in &task.relates_to {
            if removed_ids.contains(related) {
                references.push(format!("{} relates_to {}", task.id, related));
            }
        }
        if let Some(duplicate_id) = &task.duplicates
            && removed_ids.contains(duplicate_id)
        {
            references.push(format!("{} duplicates {}", task.id, duplicate_id));
        }
        if let Some(parent_id) = &task.parent_id
            && removed_ids.contains(parent_id)
        {
            references.push(format!("{} parent_id {}", task.id, parent_id));
        }
    }
    if !references.is_empty() {
        let sample = references
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "Cannot replace the existing child subtree because other tasks still reference it: {}{}",
            sample,
            if references.len() > 5 {
                format!(" (and {} more)", references.len() - 5)
            } else {
                String::new()
            }
        );
    }
    Ok(())
}

pub(super) fn insertion_index(
    active: &QueueFile,
    effective_parent: Option<&Task>,
    replaced_ids: &HashSet<String>,
    child_policy: DecompositionChildPolicy,
) -> Result<usize> {
    match effective_parent {
        None => Ok(queue::suggest_new_task_insert_index(active)),
        Some(parent) => {
            let parent_index = active
                .tasks
                .iter()
                .position(|task| task.id == parent.id)
                .with_context(|| crate::error_messages::source_task_not_found(&parent.id, false))?;
            if child_policy == DecompositionChildPolicy::Replace || replaced_ids.is_empty() {
                return Ok(parent_index + 1);
            }
            let mut max_index = parent_index;
            for (index, task) in active.tasks.iter().enumerate() {
                if replaced_ids.contains(&task.id) && index > max_index {
                    max_index = index;
                }
            }
            Ok(max_index + 1)
        }
    }
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

fn parse_numeric_id(task_id: &str, id_prefix: &str) -> Result<u32> {
    task_id
        .strip_prefix(&format!("{}-", queue::normalize_prefix(id_prefix)))
        .with_context(|| format!("task ID '{}' did not match prefix {}", task_id, id_prefix))?
        .parse::<u32>()
        .with_context(|| format!("task ID '{}' did not contain a numeric suffix", task_id))
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
