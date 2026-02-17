//! Task relationship command handlers for `ralph task` subcommands.
//!
//! Responsibilities:
//! - Handle `relate` command (generic relationship).
//! - Handle `blocks` command (mark as blocking).
//! - Handle `mark-duplicate` command (mark as duplicate).
//!
//! Not handled here:
//! - Task dependencies (handled via `edit.rs` for depends_on field).
//! - Task building or batch operations (see `build.rs`, `batch.rs`).
//!
//! Invariants/assumptions:
//! - Relationships are stored as task fields (blocks, relates_to, duplicates).
//! - For blocks and relates_to, values are appended to existing lists.
//! - For duplicates, the value is set directly (single value).

use anyhow::{Result, bail};

use crate::cli::task::args::{TaskBlocksArgs, TaskMarkDuplicateArgs, TaskRelateArgs};
use crate::config;
use crate::queue;
use crate::queue::TaskEditKey;
use crate::timeutil;

/// Handle the `relate` command (generic relationship).
pub fn handle_relate(
    args: &TaskRelateArgs,
    force: bool,
    resolved: &config::Resolved,
) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task relate", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(
        resolved,
        &format!(
            "task relate {} {} {}",
            args.task_id, args.relation, args.other_task_id
        ),
    )?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    let relation = args.relation.trim().to_lowercase();
    let edit_key = match relation.as_str() {
        "blocks" => TaskEditKey::Blocks,
        "relates_to" | "relates" => TaskEditKey::RelatesTo,
        "duplicates" | "duplicate" => TaskEditKey::Duplicates,
        _ => bail!(
            "Invalid relationship type: '{}'. Expected one of: blocks, relates_to, duplicates.",
            args.relation
        ),
    };

    // For blocks and relates_to, append to the list
    // For duplicates, set the value directly
    let value = if matches!(edit_key, TaskEditKey::Duplicates) {
        args.other_task_id.clone()
    } else {
        // Get existing values and append
        let task = queue_file
            .tasks
            .iter()
            .find(|t| t.id.trim() == args.task_id.trim())
            .ok_or_else(|| {
                anyhow::anyhow!("{}", crate::error_messages::task_not_found(&args.task_id))
            })?;

        let existing: Vec<String> = match edit_key {
            TaskEditKey::Blocks => task.blocks.clone(),
            TaskEditKey::RelatesTo => task.relates_to.clone(),
            _ => vec![],
        };

        let mut new_list = existing;
        if !new_list.contains(&args.other_task_id) {
            new_list.push(args.other_task_id.clone());
        }
        new_list.join(", ")
    };

    queue::apply_task_edit(
        &mut queue_file,
        None,
        &args.task_id,
        edit_key,
        &value,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    queue::save_queue(&resolved.queue_path, &queue_file)?;

    let relation_desc = match edit_key {
        TaskEditKey::Blocks => "blocks",
        TaskEditKey::RelatesTo => "relates to",
        TaskEditKey::Duplicates => "duplicates",
        _ => &args.relation,
    };

    log::info!(
        "Task {} now {} {}.",
        args.task_id,
        relation_desc,
        args.other_task_id
    );
    println!(
        "Task {} now {} {}.",
        args.task_id, relation_desc, args.other_task_id
    );

    Ok(())
}

/// Handle the `blocks` command (mark as blocking).
pub fn handle_blocks(
    args: &TaskBlocksArgs,
    force: bool,
    resolved: &config::Resolved,
) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task blocks", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(
        resolved,
        &format!(
            "task blocks {} -> {}",
            args.task_id,
            args.blocked_task_ids.join(", ")
        ),
    )?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Get existing blocks
    let task = queue_file
        .tasks
        .iter()
        .find(|t| t.id.trim() == args.task_id.trim())
        .ok_or_else(|| {
            anyhow::anyhow!("{}", crate::error_messages::task_not_found(&args.task_id))
        })?;

    let mut new_blocks = task.blocks.clone();
    for blocked_id in &args.blocked_task_ids {
        if !new_blocks.contains(blocked_id) {
            new_blocks.push(blocked_id.clone());
        }
    }

    let value = new_blocks.join(", ");

    queue::apply_task_edit(
        &mut queue_file,
        None,
        &args.task_id,
        TaskEditKey::Blocks,
        &value,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    queue::save_queue(&resolved.queue_path, &queue_file)?;

    log::info!(
        "Task {} now blocks: {}.",
        args.task_id,
        args.blocked_task_ids.join(", ")
    );
    println!(
        "Task {} now blocks: {}.",
        args.task_id,
        args.blocked_task_ids.join(", ")
    );

    Ok(())
}

/// Handle the `mark-duplicate` command.
pub fn handle_mark_duplicate(
    args: &TaskMarkDuplicateArgs,
    force: bool,
    resolved: &config::Resolved,
) -> Result<()> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task duplicate", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(
        resolved,
        &format!(
            "task mark-duplicate {} -> {}",
            args.task_id, args.original_task_id
        ),
    )?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    queue::apply_task_edit(
        &mut queue_file,
        None,
        &args.task_id,
        TaskEditKey::Duplicates,
        &args.original_task_id,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    queue::save_queue(&resolved.queue_path, &queue_file)?;

    log::info!(
        "Task {} marked as duplicate of {}.",
        args.task_id,
        args.original_task_id
    );
    println!(
        "Task {} marked as duplicate of {}.",
        args.task_id, args.original_task_id
    );

    Ok(())
}
