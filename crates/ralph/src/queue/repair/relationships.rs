//! Purpose: Rewrite cross-task relationship references after ID remapping.
//!
//! Responsibilities:
//! - Apply remapped task IDs to `depends_on`, `blocks`, `relates_to`,
//!   `duplicates`, and `parent_id` fields on a single task.
//! - Provide narrowly scoped helpers for list and `Option<String>` ID rewrites.
//!
//! Scope:
//! - In-memory rewrites only; never reads or writes queue files.
//! - Does not decide which IDs are remapped; callers supply the remap table.
//!
//! Usage:
//! - Used by `crate::queue::repair::planning` after the planner remaps duplicate
//!   or invalid task IDs so cross-task references stay consistent.
//!
//! Invariants/Assumptions:
//! - Remap lookups try the raw ID first and then the trimmed ID, mirroring the
//!   planner's collision detection (which trims and uppercases for keys but
//!   stores the canonical new ID under the original spelling).
//! - Returns `true` only when at least one field on the task changed.

use crate::contracts::Task;
use std::collections::HashMap;

pub(super) fn rewrite_task_id_references(
    task: &mut Task,
    remapped_ids: &HashMap<String, String>,
) -> bool {
    let mut modified = false;
    modified |= rewrite_id_list(&mut task.depends_on, remapped_ids);
    modified |= rewrite_id_list(&mut task.blocks, remapped_ids);
    modified |= rewrite_id_list(&mut task.relates_to, remapped_ids);
    modified |= rewrite_optional_id(&mut task.duplicates, remapped_ids);
    modified |= rewrite_optional_id(&mut task.parent_id, remapped_ids);
    modified
}

fn rewrite_id_list(ids: &mut [String], remapped_ids: &HashMap<String, String>) -> bool {
    let mut modified = false;
    for id in ids {
        if let Some(new_id) = remapped_task_id(id, remapped_ids) {
            *id = new_id;
            modified = true;
        }
    }
    modified
}

fn rewrite_optional_id(id: &mut Option<String>, remapped_ids: &HashMap<String, String>) -> bool {
    let Some(current_id) = id.as_deref() else {
        return false;
    };
    let Some(new_id) = remapped_task_id(current_id, remapped_ids) else {
        return false;
    };

    *id = Some(new_id);
    true
}

fn remapped_task_id(id: &str, remapped_ids: &HashMap<String, String>) -> Option<String> {
    remapped_ids
        .get(id)
        .or_else(|| remapped_ids.get(id.trim()))
        .cloned()
}
