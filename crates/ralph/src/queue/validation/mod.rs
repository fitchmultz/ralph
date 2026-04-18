//! Queue validation facade.
//!
//! Responsibilities:
//! - Expose queue validation entrypoints and warning types.
//! - Coordinate queue-file checks with dependency, relationship, and parent validators.
//! - Keep shared validation data structures local to this module tree.
//!
//! Not handled here:
//! - Queue persistence or mutation workflows.
//! - Config validation or CLI reporting.
//!
//! Invariants/assumptions:
//! - Task IDs are unique within a queue file and across active/done files, except rejected tasks.
//! - Dependency and blocking graphs must remain acyclic.
//! - Warnings are non-blocking and may be logged separately by callers.

mod dependency_graph;
mod parent;
mod queue_files;
mod relationships;
mod shared;
mod task_fields;

use anyhow::Result;

pub(crate) use task_fields::validate_task_id;

/// Represents a validation issue that doesn't prevent queue operation but should be reported.
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub task_id: String,
    pub message: String,
}

impl ValidationWarning {
    /// Log this warning using the standard logging framework.
    pub fn log(&self) {
        log::warn!("[{}] {}", self.task_id, self.message);
    }
}

/// Log all validation warnings.
pub fn log_warnings(warnings: &[ValidationWarning]) {
    for warning in warnings {
        warning.log();
    }
}

/// Result of dependency and relationship validation containing warnings.
#[derive(Debug, Default)]
pub struct DependencyValidationResult {
    pub warnings: Vec<ValidationWarning>,
}

/// Validate a single queue file.
pub fn validate_queue(
    queue: &crate::contracts::QueueFile,
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    queue_files::validate_queue(queue, id_prefix, id_width)
}

/// Validate an optional done archive file.
pub(crate) fn validate_done_queue(
    done: Option<&crate::contracts::QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    queue_files::validate_done_queue(done, id_prefix, id_width)
}

/// Validate active and optional done queues together, returning non-blocking warnings.
pub fn validate_queue_set(
    active: &crate::contracts::QueueFile,
    done: Option<&crate::contracts::QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<Vec<ValidationWarning>> {
    queue_files::validate_queue(active, id_prefix, id_width)?;
    queue_files::validate_done_queue(done, id_prefix, id_width)?;
    queue_files::validate_cross_file_duplicates(active, done)?;

    let catalog = shared::TaskCatalog::new(active, done);
    let mut result = DependencyValidationResult::default();

    dependency_graph::validate_dependency_graph(&catalog, max_dependency_depth, &mut result)?;
    relationships::validate_relationships(&catalog, &mut result)?;
    parent::validate_parent_ids(&catalog, &mut result)?;

    Ok(result.warnings)
}

#[cfg(test)]
mod tests;
