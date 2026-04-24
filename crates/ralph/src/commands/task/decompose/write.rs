//! Queue-write orchestration for task decomposition.
//!
//! Purpose:
//! - Queue-write orchestration for task decomposition.
//!
//! Responsibilities:
//! - Re-validate queue state under lock before persisting decomposition output.
//! - Enforce child-policy semantics and create undo snapshots for durable writes.
//! - Materialize normalized planner trees into queue tasks with stable insertion order.
//!
//! Not handled here:
//! - Planner execution, prompt rendering, or response parsing.
//! - Tree normalization algorithms or low-level helper implementations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Writes must fail fast when preview blockers remain unresolved.
//! - Queue validation runs both before and after mutation.

use super::resolve::resolve_effective_parent_for_write;
use super::support::{
    allocate_sequential_ids, annotate_parent, created_node_count, descendant_ids_for_parent,
    done_queue_ref, ensure_subtree_is_replaceable, insertion_index, materialize_children,
    materialize_node, request_context,
};
use super::types::{
    DecompositionChildPolicy, DecompositionPreview, DecompositionSource, TaskDecomposeWriteResult,
};
use crate::contracts::QueueFile;
use crate::{config, queue, timeutil};
use anyhow::{Context, Result, bail};

pub fn write_task_decomposition(
    resolved: &config::Resolved,
    preview: &DecompositionPreview,
    force: bool,
) -> Result<TaskDecomposeWriteResult> {
    if !preview.write_blockers.is_empty() {
        bail!(preview.write_blockers.join("\n"));
    }

    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task decompose", force)?;
    let mut active = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = done_queue_ref(&done, &resolved.done_path);
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    validate_queue_set(&active, done_ref, resolved, max_depth)
        .context("validate queue set before task decompose write")?;

    let effective_parent = resolve_effective_parent_for_write(&active, done_ref, preview)?;
    let existing_descendant_ids = effective_parent
        .as_ref()
        .map(|task| descendant_ids_for_parent(&active, task.id.as_str()))
        .transpose()?
        .unwrap_or_default();

    enforce_child_policy(
        preview,
        effective_parent.as_ref(),
        &active,
        done_ref,
        &existing_descendant_ids,
    )?;
    crate::undo::create_undo_snapshot(resolved, &undo_snapshot_label(preview))?;

    let created_count = created_node_count(preview);
    if created_count == 0 {
        bail!("Task decomposition produced no child tasks to write.");
    }

    let ids = allocate_sequential_ids(
        &active,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
        created_count,
    )?;
    let now = timeutil::now_utc_rfc3339()?;
    let request_context = request_context(preview);
    let mut next_id_index = 0usize;
    let mut created_tasks = materialize_created_tasks(
        preview,
        effective_parent.as_ref(),
        &ids,
        &mut next_id_index,
        &request_context,
        &now,
    )?;

    let root_task_id = match (&preview.source, preview.attach_target.as_ref()) {
        (DecompositionSource::ExistingTask { .. }, None) => None,
        _ => created_tasks.first().map(|task| task.id.clone()),
    };
    let parent_task_id = effective_parent.as_ref().map(|task| task.id.clone());
    let created_ids = created_tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let replaced_ids = if preview.child_policy == DecompositionChildPolicy::Replace {
        existing_descendant_ids.iter().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let removed_ids = existing_descendant_ids;
    if !removed_ids.is_empty() && preview.child_policy == DecompositionChildPolicy::Replace {
        active
            .tasks
            .retain(|task| !removed_ids.contains(task.id.as_str()));
    }

    let insert_at = insertion_index(
        &active,
        effective_parent.as_ref(),
        &removed_ids,
        preview.child_policy,
    )?;

    if let Some(parent) = effective_parent {
        annotate_parent(
            &mut active,
            &parent.id,
            &preview.source,
            preview.attach_target.as_ref(),
            &created_tasks,
            &now,
        )?;
    }

    for (offset, task) in created_tasks.drain(..).enumerate() {
        active.tasks.insert(insert_at + offset, task);
    }

    validate_queue_set(&active, done_ref, resolved, max_depth)
        .context("validate queue set after task decompose write")?;
    queue::save_queue(&resolved.queue_path, &active)?;

    Ok(TaskDecomposeWriteResult {
        root_task_id,
        parent_task_id,
        created_ids,
        replaced_ids,
        parent_annotated: preview.attach_target.is_some()
            || matches!(preview.source, DecompositionSource::ExistingTask { .. }),
    })
}

fn validate_queue_set(
    active: &QueueFile,
    done_ref: Option<&QueueFile>,
    resolved: &config::Resolved,
    max_depth: u8,
) -> Result<()> {
    queue::validate_queue_set(
        active,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .map(|_| ())
}

fn enforce_child_policy(
    preview: &DecompositionPreview,
    effective_parent: Option<&crate::contracts::Task>,
    active: &QueueFile,
    done_ref: Option<&QueueFile>,
    existing_descendant_ids: &std::collections::HashSet<String>,
) -> Result<()> {
    match preview.child_policy {
        DecompositionChildPolicy::Fail => {
            if !existing_descendant_ids.is_empty() {
                let parent_id = effective_parent
                    .as_ref()
                    .map(|task| task.id.as_str())
                    .unwrap_or("");
                bail!(
                    "Task {} already has child tasks. Refusing write for `ralph task decompose --child-policy fail`.",
                    parent_id
                );
            }
        }
        DecompositionChildPolicy::Replace => {
            if !existing_descendant_ids.is_empty() {
                ensure_subtree_is_replaceable(active, done_ref, existing_descendant_ids)?;
            }
        }
        DecompositionChildPolicy::Append => {}
    }
    Ok(())
}

fn materialize_created_tasks(
    preview: &DecompositionPreview,
    effective_parent: Option<&crate::contracts::Task>,
    ids: &[String],
    next_id_index: &mut usize,
    request_context: &str,
    now: &str,
) -> Result<Vec<crate::contracts::Task>> {
    match (&preview.source, effective_parent) {
        (DecompositionSource::ExistingTask { .. }, Some(parent))
            if preview.attach_target.is_none() =>
        {
            materialize_children(
                &preview.plan.root.children,
                Some(parent.id.as_str()),
                ids,
                next_id_index,
                preview.child_status,
                request_context,
                now,
            )
        }
        (_, Some(parent)) => {
            let root_task = materialize_node(
                &preview.plan.root,
                Some(parent.id.as_str()),
                ids,
                next_id_index,
                preview.child_status,
                request_context,
                now,
            )?;
            let root_id = root_task.id.clone();
            let mut tasks = vec![root_task];
            tasks.extend(materialize_children(
                &preview.plan.root.children,
                Some(root_id.as_str()),
                ids,
                next_id_index,
                preview.child_status,
                request_context,
                now,
            )?);
            Ok(tasks)
        }
        (_, None) => {
            let root_task = materialize_node(
                &preview.plan.root,
                None,
                ids,
                next_id_index,
                preview.child_status,
                request_context,
                now,
            )?;
            let root_id = root_task.id.clone();
            let mut tasks = vec![root_task];
            tasks.extend(materialize_children(
                &preview.plan.root.children,
                Some(root_id.as_str()),
                ids,
                next_id_index,
                preview.child_status,
                request_context,
                now,
            )?);
            Ok(tasks)
        }
    }
}

fn undo_snapshot_label(preview: &DecompositionPreview) -> String {
    match (&preview.source, preview.attach_target.as_ref()) {
        (DecompositionSource::Freeform { request }, None) => {
            format!("task decompose write for request '{request}'")
        }
        (DecompositionSource::Freeform { request }, Some(parent)) => {
            format!(
                "task decompose attach request '{}' under {}",
                request, parent.task.id
            )
        }
        (DecompositionSource::ExistingTask { task }, None) => {
            format!("task decompose {} into child tasks", task.id)
        }
        (DecompositionSource::ExistingTask { task }, Some(parent)) => {
            format!(
                "task decompose {} attached under {}",
                task.id, parent.task.id
            )
        }
    }
}
