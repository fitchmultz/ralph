//! Purpose: Undo-backed apply, validation, and persistence for queue repair.
//!
//! Responsibilities:
//! - Plan a repair, then validate, snapshot undo, and save when changes exist.
//! - Provide the public `apply_queue_repair_with_undo` and
//!   `apply_queue_maintenance_repair_with_undo` entry points.
//! - Validate planned mutations against the queue-set rules before saving.
//! - Persist active and done queue files only when their respective halves of
//!   the plan changed.
//!
//! Scope:
//! - Apply orchestration only; planning lives in `planning.rs`.
//! - Lock acquisition lives in `crate::queue` callers; this module assumes the
//!   caller already holds the queue lock.
//!
//! Usage:
//! - CLI, machine, and doctor recovery surfaces call the apply entry points to
//!   normalize recoverable queue inconsistencies safely.
//!
//! Invariants/Assumptions:
//! - No-change plans short-circuit before any validation, undo snapshot, or save.
//! - When changes exist, validation runs before the undo snapshot is taken so
//!   bad plans cannot rewrite the queue.
//! - Saves are gated by per-half change flags so unmodified files stay untouched.

use super::planning::{plan_queue_maintenance_repair, plan_queue_repair};
use super::types::{QueueRepairPlan, RepairReport};
use crate::config::Resolved;
use crate::constants::queue::DEFAULT_MAX_DEPENDENCY_DEPTH;
use crate::lock::DirLock;
use crate::queue::{save_queue, validation};
use anyhow::Result;
use std::path::Path;

pub fn apply_queue_repair_with_undo(
    resolved: &Resolved,
    _queue_lock: &DirLock,
    operation: &str,
) -> Result<RepairReport> {
    apply_repair_plan_with_undo(
        resolved,
        operation,
        plan_queue_repair(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?,
    )
}

pub fn apply_queue_maintenance_repair_with_undo(
    resolved: &Resolved,
    _queue_lock: &DirLock,
    operation: &str,
) -> Result<RepairReport> {
    apply_repair_plan_with_undo(
        resolved,
        operation,
        plan_queue_maintenance_repair(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?,
    )
}

fn apply_repair_plan_with_undo(
    resolved: &Resolved,
    operation: &str,
    plan: QueueRepairPlan,
) -> Result<RepairReport> {
    let report = plan.report().clone();

    if !plan.has_changes() {
        return Ok(report);
    }

    validate_repair_plan(&plan, &resolved.id_prefix, resolved.id_width)?;
    crate::undo::create_undo_snapshot(resolved, operation)?;
    save_repair_plan(&resolved.queue_path, &resolved.done_path, &plan)?;
    Ok(report)
}

fn validate_repair_plan(plan: &QueueRepairPlan, id_prefix: &str, id_width: usize) -> Result<()> {
    validation::validate_queue_set(
        &plan.active,
        Some(&plan.done),
        id_prefix,
        id_width,
        DEFAULT_MAX_DEPENDENCY_DEPTH,
    )?;
    Ok(())
}

fn save_repair_plan(queue_path: &Path, done_path: &Path, plan: &QueueRepairPlan) -> Result<()> {
    if plan.queue_changed {
        save_queue(queue_path, &plan.active)?;
    }
    if plan.done_changed {
        save_queue(done_path, &plan.done)?;
    }
    Ok(())
}
