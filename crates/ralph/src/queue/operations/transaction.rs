//! Transaction-style task mutation helpers.
//!
//! Responsibilities:
//! - Define structured task-mutation requests that can apply multiple field edits atomically.
//! - Enforce optimistic-lock checks against `updated_at` when requested by callers.
//! - Reuse existing edit primitives while providing all-or-nothing mutation semantics.
//!
//! Does not handle:
//! - Queue persistence or lock acquisition.
//! - CLI argument parsing or JSON IO.
//! - Terminal archive moves across queue/done files.
//!
//! Invariants/assumptions:
//! - Requests target tasks in the active queue only.
//! - Atomic requests leave the caller's queue untouched when any mutation fails.
//! - `expected_updated_at` compares canonical RFC3339 instants, not source formatting.

use crate::contracts::QueueFile;
use crate::queue::TaskEditKey;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskMutationRequest {
    #[serde(default = "task_mutation_request_version")]
    pub version: u8,
    #[serde(default = "task_mutation_request_atomic_default")]
    pub atomic: bool,
    #[serde(default)]
    pub tasks: Vec<TaskMutationSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskMutationSpec {
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_updated_at: Option<String>,
    #[serde(default)]
    pub edits: Vec<TaskFieldEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskFieldEdit {
    pub field: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskMutationReport {
    #[serde(default = "task_mutation_request_version")]
    pub version: u8,
    pub atomic: bool,
    pub tasks: Vec<TaskMutationTaskReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskMutationTaskReport {
    pub task_id: String,
    pub applied_edits: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskMutationError {
    #[error("Task mutation request must include at least one task.")]
    EmptyRequest,
    #[error("Task mutation for {task_id} must include at least one edit.")]
    EmptyTaskEdits { task_id: String },
    #[error(
        "Task mutation conflict for {task_id}: expected updated_at {expected}, found {actual}."
    )]
    OptimisticConflict {
        task_id: String,
        expected: String,
        actual: String,
    },
    #[error(
        "Task mutation conflict for {task_id}: expected updated_at {expected}, but the task has no updated_at."
    )]
    MissingActualTimestamp { task_id: String, expected: String },
}

const fn task_mutation_request_version() -> u8 {
    1
}

const fn task_mutation_request_atomic_default() -> bool {
    true
}

#[allow(clippy::too_many_arguments)]
pub fn apply_task_mutation_request(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    request: &TaskMutationRequest,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<TaskMutationReport> {
    if request.tasks.is_empty() {
        return Err(TaskMutationError::EmptyRequest.into());
    }

    if request.atomic {
        let mut working = queue.clone();
        let report = apply_request_into_queue(
            &mut working,
            done,
            request,
            now_rfc3339,
            id_prefix,
            id_width,
            max_dependency_depth,
        )?;
        *queue = working;
        return Ok(report);
    }

    apply_request_into_queue(
        queue,
        done,
        request,
        now_rfc3339,
        id_prefix,
        id_width,
        max_dependency_depth,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_request_into_queue(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    request: &TaskMutationRequest,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<TaskMutationReport> {
    let mut reports = Vec::with_capacity(request.tasks.len());

    for task in &request.tasks {
        if task.edits.is_empty() {
            return Err(TaskMutationError::EmptyTaskEdits {
                task_id: task.task_id.trim().to_string(),
            }
            .into());
        }

        ensure_expected_updated_at(queue, task)?;

        for edit in &task.edits {
            let key = edit.field.parse::<TaskEditKey>().with_context(|| {
                format!(
                    "Invalid task mutation field '{}' for task {}",
                    edit.field, task.task_id
                )
            })?;
            super::edit::apply_task_edit(
                queue,
                done,
                &task.task_id,
                key,
                &edit.value,
                now_rfc3339,
                id_prefix,
                id_width,
                max_dependency_depth,
            )?;
        }

        reports.push(TaskMutationTaskReport {
            task_id: task.task_id.trim().to_string(),
            applied_edits: task.edits.len(),
        });
    }

    Ok(TaskMutationReport {
        version: task_mutation_request_version(),
        atomic: request.atomic,
        tasks: reports,
    })
}

fn ensure_expected_updated_at(queue: &QueueFile, task: &TaskMutationSpec) -> Result<()> {
    let Some(expected) = task.expected_updated_at.as_ref() else {
        return Ok(());
    };

    let task_id = task.task_id.trim();
    if task_id.is_empty() {
        bail!("Task mutation is missing task_id.");
    }

    let current = queue
        .tasks
        .iter()
        .find(|candidate| candidate.id.trim() == task_id)
        .ok_or_else(|| anyhow!("{}", crate::error_messages::task_not_found(task_id)))?;

    let expected_trimmed = expected.trim();
    let expected_dt = crate::timeutil::parse_rfc3339(expected_trimmed)
        .with_context(|| format!("parse expected updated_at for task {}", task_id))?;

    match current.updated_at.as_deref().map(str::trim) {
        Some(actual)
            if crate::timeutil::parse_rfc3339(actual)
                .map(|actual_dt| actual_dt == expected_dt)
                .unwrap_or(false) =>
        {
            Ok(())
        }
        Some(actual) => Err(TaskMutationError::OptimisticConflict {
            task_id: task_id.to_string(),
            expected: expected_trimmed.to_string(),
            actual: actual.to_string(),
        }
        .into()),
        None => Err(TaskMutationError::MissingActualTimestamp {
            task_id: task_id.to_string(),
            expected: expected_trimmed.to_string(),
        }
        .into()),
    }
}
