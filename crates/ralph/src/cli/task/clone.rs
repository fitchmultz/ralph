//! Task cloning command handler for `ralph task clone` subcommand.
//!
//! Responsibilities:
//! - Handle `clone` command (duplicate an existing task).
//! - Support `duplicate` alias.
//! - Support dry-run mode for previewing clones.
//!
//! Not handled here:
//! - Task building or batch operations (see `build.rs`, `batch.rs`).
//! - Template-based task creation (see `template.rs`).
//!
//! Invariants/assumptions:
//! - Source task can be from queue or done archive.
//! - Cloned task gets a new ID and can have modified status/title.
//! - New task is inserted at the appropriate position in the queue.

use anyhow::Result;

use crate::cli::task::args::{TaskCloneArgs, TaskStatusArg};
use crate::config;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::timeutil;

/// Handle the `clone` command.
pub fn handle(args: &TaskCloneArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    let status: TaskStatus = args.status.unwrap_or(TaskStatusArg::Draft).into();

    // Load both queue and done files
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };

    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Build clone options
    let clone_opts = queue::operations::CloneTaskOptions::new(
        &args.task_id,
        status,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_title_prefix(args.title_prefix.as_deref())
    .with_max_depth(max_depth);

    // Perform the clone operation
    let (new_id, cloned_task) = queue::operations::clone_task(
        &mut queue_file.clone(), // Clone for dry run check
        done_ref,
        &clone_opts,
    )?;

    if args.dry_run {
        println!(
            "Dry run - would clone task {} to new task {}:",
            args.task_id, new_id
        );
        println!("  Title: {}", cloned_task.title);
        println!("  Status: {}", cloned_task.status);
        println!("  Priority: {}", cloned_task.priority);
        if !cloned_task.tags.is_empty() {
            println!("  Tags: {}", cloned_task.tags.join(", "));
        }
        if !cloned_task.scope.is_empty() {
            println!("  Scope: {}", cloned_task.scope.join(", "));
        }
        return Ok(());
    }

    // Acquire lock and perform actual clone
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task clone", force)?;
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let (new_id, cloned_task) =
        queue::operations::clone_task(&mut queue_file, done_ref, &clone_opts)?;

    // Insert at appropriate position
    let insert_at = queue::operations::suggest_new_task_insert_index(&queue_file);
    queue_file.tasks.insert(insert_at, cloned_task);

    // Save queue
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    log::info!(
        "Cloned task {} to new task {} (status: {})",
        args.task_id,
        new_id,
        status
    );
    println!("Created new task {} from clone of {}", new_id, args.task_id);

    Ok(())
}
