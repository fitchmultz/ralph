//! Batch operation regression test hub.
//!
//! Purpose:
//! - Batch operation regression test hub.
//!
//! Responsibilities:
//! - Share batch-operation test imports and fixtures across focused submodules.
//! - Keep the root test surface small while delegating behavior groups to companion files.
//!
//! Non-scope:
//! - Single-task operation coverage from sibling test modules.
//! - Queue persistence or broader integration flows.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Shared fixtures from `super` provide standard task construction.
//! - Batch-operation helpers are imported from the production `batch` module.

use super::*;
use crate::contracts::TaskStatus;
use crate::queue::operations::batch::{
    BatchOperationResult, batch_apply_edit, batch_set_field, batch_set_status, collect_task_ids,
    deduplicate_task_ids, filter_tasks_by_tags, resolve_task_ids,
};
use crate::queue::operations::edit::TaskEditKey;

mod basic;
mod edge_cases;
