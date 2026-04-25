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
    annotate_parent, created_node_count, descendant_ids_for_parent, done_queue_ref,
    materialized_specs_for_preview, request_context,
};
use super::types::{
    DecompositionChildPolicy, DecompositionPreview, DecompositionSource, TaskDecomposeWriteResult,
};
use crate::contracts::QueueFile;
use crate::queue::operations::{
    MaterializeInsertion, MaterializeTaskGraphOptions, apply_materialized_task_graph,
    ensure_subtree_is_replaceable,
};
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

    let now = timeutil::now_utc_rfc3339()?;
    let request_context = request_context(preview);
    let specs =
        materialized_specs_for_preview(preview, effective_parent.as_ref(), &request_context);
    let materialize_insertion = materialize_insertion_strategy(
        preview,
        effective_parent.as_ref(),
        &existing_descendant_ids,
    )?;

    let parent_task_id = effective_parent.as_ref().map(|task| task.id.clone());
    let replaced_ids = if preview.child_policy == DecompositionChildPolicy::Replace {
        existing_descendant_ids.iter().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let materialized = apply_materialized_task_graph(
        &mut active,
        done_ref,
        &specs,
        &MaterializeTaskGraphOptions {
            now_rfc3339: &now,
            id_prefix: &resolved.id_prefix,
            id_width: resolved.id_width,
            max_dependency_depth: max_depth,
            insertion: materialize_insertion,
            dry_run: false,
        },
    )
    .context("materialize task decomposition queue updates")?;

    if let Some(parent) = effective_parent.as_ref() {
        annotate_parent(
            &mut active,
            &parent.id,
            &preview.source,
            preview.attach_target.as_ref(),
            &materialized.created_tasks,
            &now,
        )?;
    }

    queue::save_queue(&resolved.queue_path, &active)?;
    let root_task_id = match (&preview.source, preview.attach_target.as_ref()) {
        (DecompositionSource::ExistingTask { .. }, None) => None,
        _ => materialized
            .created_tasks
            .first()
            .map(|task| task.id.clone()),
    };
    let created_ids = materialized
        .created_tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();

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

fn materialize_insertion_strategy(
    preview: &DecompositionPreview,
    effective_parent: Option<&crate::contracts::Task>,
    existing_descendant_ids: &std::collections::HashSet<String>,
) -> Result<MaterializeInsertion> {
    Ok(match effective_parent {
        None => MaterializeInsertion::QueueDefaultTop,
        Some(parent) if preview.child_policy == DecompositionChildPolicy::Replace => {
            MaterializeInsertion::ReplaceSubtree {
                parent_task_id: parent.id.clone(),
                removed_subtree_task_ids: existing_descendant_ids.iter().cloned().collect(),
            }
        }
        Some(parent) if existing_descendant_ids.is_empty() => MaterializeInsertion::AfterParent {
            parent_task_id: parent.id.clone(),
        },
        Some(parent) => MaterializeInsertion::AppendUnderParent {
            parent_task_id: parent.id.clone(),
            existing_subtree_task_ids: existing_descendant_ids.iter().cloned().collect(),
        },
    })
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
