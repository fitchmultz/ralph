//! Selection helpers for `ralph task batch`.
//!
//! Purpose:
//! - Selection helpers for `ralph task batch`.
//!
//! Responsibilities:
//! - Translate CLI filters into core queue batch filters.
//! - Resolve target task IDs from explicit IDs or selector filters.
//! - Enforce non-empty selection for mutating operations.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::cli::task::args::BatchSelectArgs;
use crate::contracts::QueueFile;
use crate::queue;
use anyhow::{Result, bail};

pub(super) fn resolve_with_filters(
    queue_file: &QueueFile,
    select: &BatchSelectArgs,
    now_rfc3339: &str,
) -> Result<Vec<String>> {
    let filters = queue::operations::BatchTaskFilters {
        status_filter: select
            .status_filter
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
        priority_filter: select
            .priority_filter
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
        scope_filter: select.scope_filter.clone(),
        older_than: select.older_than.clone(),
    };

    queue::operations::resolve_task_ids_filtered(
        queue_file,
        &select.task_ids,
        &select.tag_filter,
        &filters,
        now_rfc3339,
    )
}

pub(super) fn require_task_ids(task_ids: Vec<String>) -> Result<Vec<String>> {
    if task_ids.is_empty() {
        bail!("No tasks specified. Provide task IDs or use filters.");
    }
    Ok(task_ids)
}
