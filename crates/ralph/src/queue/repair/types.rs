//! Purpose: Data models for queue repair planning.
//!
//! Responsibilities:
//! - Describe the user-visible repair report (`RepairReport`).
//! - Describe the planned repair mutation (`QueueRepairPlan`) that apply flows consume.
//! - Encode the planner scope (`RepairScope`) shared between maintenance and
//!   full repair planning.
//!
//! Scope:
//! - Pure data plus tiny accessors; no IO, validation, or planning logic.
//!
//! Usage:
//! - `crate::queue::repair::planning` builds `QueueRepairPlan` and `RepairReport`.
//! - `crate::queue::repair::apply` consumes `QueueRepairPlan` to validate and persist.
//!
//! Invariants/Assumptions:
//! - `QueueRepairPlan::has_changes` is the single source of truth for whether
//!   apply must validate, snapshot undo, and save.
//! - Field visibility stays `pub(super)` for plan internals so only the repair
//!   facade can construct or mutate plan state.

use crate::contracts::QueueFile;
use serde::Serialize;

#[derive(Debug, Default, Clone, Serialize)]
pub struct RepairReport {
    pub fixed_tasks: usize,
    pub remapped_ids: Vec<(String, String)>,
    pub fixed_timestamps: usize,
}

impl RepairReport {
    pub fn is_empty(&self) -> bool {
        self.fixed_tasks == 0 && self.remapped_ids.is_empty() && self.fixed_timestamps == 0
    }
}

#[derive(Debug, Clone)]
pub struct QueueRepairPlan {
    pub(super) active: QueueFile,
    pub(super) done: QueueFile,
    pub(super) report: RepairReport,
    pub(super) queue_changed: bool,
    pub(super) done_changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RepairScope {
    Maintenance,
    Full,
}

impl QueueRepairPlan {
    pub fn has_changes(&self) -> bool {
        self.queue_changed || self.done_changed
    }

    pub fn report(&self) -> &RepairReport {
        &self.report
    }

    pub fn into_parts(self) -> (QueueFile, QueueFile, RepairReport) {
        (self.active, self.done, self.report)
    }
}
