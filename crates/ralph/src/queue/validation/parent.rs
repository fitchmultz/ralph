//! Queue parent hierarchy validation.
//!
//! Purpose:
//! - Queue parent hierarchy validation.
//!
//! Responsibilities:
//! - Validate `parent_id` references and emit non-blocking orphan/self-parent warnings.
//! - Reject multi-node parent cycles.
//!
//! Not handled here:
//! - Dependency or relationship fields outside `parent_id`.
//! - Parent repair or mutation flows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Empty or whitespace-only parent IDs are treated as unset.
//! - Self-parenting is warned, while longer parent cycles are rejected.

use super::{DependencyValidationResult, ValidationWarning, shared::TaskCatalog};
use anyhow::{Result, bail};

pub(crate) fn validate_parent_ids(
    catalog: &TaskCatalog<'_>,
    result: &mut DependencyValidationResult,
) -> Result<()> {
    for task in &catalog.tasks {
        let task_id = task.id.trim();
        if task_id.is_empty() {
            continue;
        }

        let Some(parent_id) = task.parent_id.as_deref() else {
            continue;
        };
        let parent_id = parent_id.trim();
        if parent_id.is_empty() {
            continue;
        }

        if parent_id == task_id {
            result.warnings.push(ValidationWarning {
                task_id: task_id.to_string(),
                message: format!(
                    "Task {} references itself as its own parent. Remove the parent_id or set it to a valid parent task.",
                    task_id
                ),
            });
            continue;
        }

        if !catalog.all_task_ids.contains(parent_id) {
            result.warnings.push(ValidationWarning {
                task_id: task_id.to_string(),
                message: format!(
                    "Task {} references parent {} which does not exist in the queue or done archive.",
                    task_id, parent_id
                ),
            });
        }
    }

    let cycles: Vec<_> = crate::queue::hierarchy::detect_parent_cycles(&catalog.tasks)
        .into_iter()
        .filter(|cycle| cycle.len() > 1)
        .collect();
    if let Some(cycle) = cycles.first() {
        bail!(
            "Circular parent chain detected: {}. Task parent_id relationships must form a DAG (no cycles). Break the cycle by changing one of the parent_id references.",
            cycle.join(" -> ")
        );
    }

    Ok(())
}
