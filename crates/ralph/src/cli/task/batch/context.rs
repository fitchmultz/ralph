//! Shared context and mutation helpers for batch task CLI operations.
//!
//! Purpose:
//! - Shared context and mutation helpers for batch task CLI operations.
//!
//! Responsibilities:
//! - Load queue/done state and shared batch metadata.
//! - Acquire queue locks and create undo snapshots for mutating operations.
//! - Persist updated queue/done files after batch mutations.
//!
//! Non-scope:
//! - Operation-specific queue mutations.
//! - Dry-run rendering or task selection.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::config;
use crate::contracts::QueueFile;
use crate::lock;
use crate::queue;
use crate::timeutil;
use anyhow::Result;

pub(super) struct BatchContext<'a> {
    pub resolved: &'a config::Resolved,
    pub queue_file: QueueFile,
    pub done_file: QueueFile,
    pub now: String,
    pub max_depth: u8,
}

impl<'a> BatchContext<'a> {
    pub(super) fn load(resolved: &'a config::Resolved) -> Result<Self> {
        Ok(Self {
            resolved,
            queue_file: queue::load_queue(&resolved.queue_path)?,
            done_file: queue::load_queue_or_default(&resolved.done_path)?,
            now: timeutil::now_utc_rfc3339()?,
            max_depth: resolved.config.queue.max_dependency_depth.unwrap_or(10),
        })
    }

    pub(super) fn done_ref(&self) -> Option<&QueueFile> {
        if self.done_file.tasks.is_empty() && !self.resolved.done_path.exists() {
            None
        } else {
            Some(&self.done_file)
        }
    }

    pub(super) fn begin_mutation(
        &self,
        force: bool,
        snapshot_label: &str,
    ) -> Result<lock::DirLock> {
        let queue_lock = queue::acquire_queue_lock(&self.resolved.repo_root, "task batch", force)?;
        crate::undo::create_undo_snapshot(self.resolved, snapshot_label)?;
        Ok(queue_lock)
    }

    pub(super) fn reload_queue(&self) -> Result<QueueFile> {
        queue::load_queue(&self.resolved.queue_path)
    }

    pub(super) fn reload_done(&self) -> Result<QueueFile> {
        queue::load_queue_or_default(&self.resolved.done_path)
    }

    pub(super) fn save_queue(&self, queue_file: &QueueFile) -> Result<()> {
        queue::save_queue(&self.resolved.queue_path, queue_file)
    }

    pub(super) fn save_done(&self, done_file: &QueueFile) -> Result<()> {
        queue::save_queue(&self.resolved.done_path, done_file)
    }
}
