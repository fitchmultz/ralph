//! Source and attach-target resolution helpers for task decomposition.
//!
//! Purpose:
//! - Source and attach-target resolution helpers for task decomposition.
//!
//! Responsibilities:
//! - Resolve freeform versus existing-task decomposition sources.
//! - Validate optional attach targets and effective parents before writes.
//! - Compute preview-time write blockers for child-policy enforcement.
//!
//! Not handled here:
//! - Planner invocation, prompt construction, or response parsing.
//! - Queue mutation, undo snapshots, or task materialization.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Existing-task decomposition only operates on active, non-terminal tasks.
//! - Attach targets are restricted to freeform sources and active queue tasks.

use super::support::{descendant_ids_for_parent, looks_like_task_id};
use super::types::{
    DecompositionAttachTarget, DecompositionChildPolicy, DecompositionPreview, DecompositionSource,
};
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue::operations::ensure_subtree_is_replaceable;
use crate::{config, queue};
use anyhow::{Context, Result, bail};

pub(super) fn resolve_source(
    resolved: &config::Resolved,
    active: &QueueFile,
    done: Option<&QueueFile>,
    source_input: &str,
) -> Result<DecompositionSource> {
    if source_input.is_empty() {
        bail!("Missing source: task decompose requires a task ID or freeform request.");
    }
    if looks_like_task_id(source_input, &resolved.id_prefix, resolved.id_width) {
        let task = queue::operations::find_task_across(active, done, source_input)
            .with_context(|| format!("Unknown task ID '{source_input}' for task decomposition."))?;
        if done.is_some_and(|done_file| {
            queue::operations::find_task(done_file, source_input).is_some()
        }) {
            bail!(
                "Task {} is in the done archive. `ralph task decompose` only supports active tasks unless explicitly overridden.",
                source_input
            );
        }
        ensure_existing_task_is_supported(task)?;
        return Ok(DecompositionSource::ExistingTask {
            task: Box::new(task.clone()),
        });
    }

    Ok(DecompositionSource::Freeform {
        request: source_input.to_string(),
    })
}

pub(super) fn resolve_attach_target(
    resolved: &config::Resolved,
    active: &QueueFile,
    done: Option<&QueueFile>,
    attach_to: Option<&str>,
    source: &DecompositionSource,
) -> Result<Option<DecompositionAttachTarget>> {
    let Some(attach_to) = attach_to.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if !looks_like_task_id(attach_to, &resolved.id_prefix, resolved.id_width) {
        bail!(
            "Invalid attach target '{}': expected a task ID like {}-0001.",
            attach_to,
            queue::normalize_prefix(&resolved.id_prefix)
        );
    }
    if matches!(source, DecompositionSource::ExistingTask { .. }) {
        bail!(
            "`ralph task decompose --attach-to` only supports freeform request sources. Use either an existing task source or --attach-to, not both."
        );
    }
    let task = queue::operations::find_task_across(active, done, attach_to)
        .with_context(|| format!("Unknown attach target '{attach_to}' for task decomposition."))?;
    if done.is_some_and(|done_file| queue::operations::find_task(done_file, attach_to).is_some()) {
        bail!(
            "Task {} is in the done archive and cannot be used as an attach target.",
            attach_to
        );
    }
    ensure_existing_task_is_supported(task)?;
    let hierarchy = queue::hierarchy::HierarchyIndex::build(active, done);
    Ok(Some(DecompositionAttachTarget {
        task: Box::new(task.clone()),
        has_existing_children: !hierarchy.children_of(&task.id).is_empty(),
    }))
}

pub(super) fn resolve_effective_parent_for_write(
    active: &QueueFile,
    done: Option<&QueueFile>,
    preview: &DecompositionPreview,
) -> Result<Option<Task>> {
    if let Some(attach_target) = &preview.attach_target {
        let task =
            queue::operations::find_task(active, &attach_target.task.id).with_context(|| {
                crate::error_messages::source_task_not_found(&attach_target.task.id, false)
            })?;
        ensure_existing_task_is_supported(task)?;
        return Ok(Some(task.clone()));
    }

    match &preview.source {
        DecompositionSource::Freeform { .. } => Ok(None),
        DecompositionSource::ExistingTask { task } => {
            let active_task = queue::operations::find_task(active, &task.id)
                .with_context(|| crate::error_messages::source_task_not_found(&task.id, false))?;
            if done.is_some_and(|done_file| {
                queue::operations::find_task(done_file, &task.id).is_some()
            }) {
                bail!(
                    "Task {} is in the done archive and cannot be decomposed in-place.",
                    task.id
                );
            }
            ensure_existing_task_is_supported(active_task)?;
            Ok(Some(active_task.clone()))
        }
    }
}

pub(super) fn compute_write_blockers(
    active: &QueueFile,
    done: Option<&QueueFile>,
    source: &DecompositionSource,
    attach_target: Option<&DecompositionAttachTarget>,
    child_policy: DecompositionChildPolicy,
) -> Result<Vec<String>> {
    let mut write_blockers = Vec::new();
    let effective_parent_id = attach_target
        .map(|target| target.task.id.clone())
        .or_else(|| match source {
            DecompositionSource::ExistingTask { task } => Some(task.id.clone()),
            DecompositionSource::Freeform { .. } => None,
        });

    if let Some(parent_id) = effective_parent_id {
        let descendant_ids = descendant_ids_for_parent(active, parent_id.as_str())?;
        let has_existing_children = !descendant_ids.is_empty();
        match child_policy {
            DecompositionChildPolicy::Fail if has_existing_children => {
                write_blockers.push(format!(
                    "Write blocked: task {} already has child tasks and --child-policy is set to fail.",
                    parent_id
                ));
            }
            DecompositionChildPolicy::Replace if has_existing_children => {
                if let Err(err) = ensure_subtree_is_replaceable(active, done, &descendant_ids) {
                    write_blockers.push(err.to_string());
                }
            }
            _ => {}
        }
    }
    Ok(write_blockers)
}

pub(super) fn ensure_existing_task_is_supported(task: &Task) -> Result<()> {
    if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
        bail!(
            "Task {} has terminal status {} and cannot be decomposed without an explicit override.",
            task.id,
            task.status
        );
    }
    Ok(())
}
