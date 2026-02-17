//! Task splitting command handler for `ralph task split` subcommand.
//!
//! Responsibilities:
//! - Handle `split` command (break down a task into child tasks).
//! - Support automatic plan distribution across child tasks.
//! - Mark original task as split via custom field.
//!
//! Not handled here:
//! - Queue persistence and locking semantics (see `crate::queue` and `crate::lock`).
//! - Task cloning for non-split scenarios (see `clone.rs`).
//!
//! Invariants/assumptions:
//! - Source task must exist in queue (not done archive).
//! - Child tasks get parent_id set to source task ID.
//! - Original task is marked with custom field `split: true`.
//! - Plan items can be distributed evenly across child tasks.

use anyhow::{Context, Result, bail};

use crate::cli::task::args::{TaskSplitArgs, TaskStatusArg};
use crate::config;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::timeutil;

/// Handle the `split` command.
pub fn handle(args: &TaskSplitArgs, force: bool, resolved: &config::Resolved) -> Result<()> {
    // Validate number >= 2 (splitting into 1 task is meaningless)
    if args.number < 2 {
        bail!("Number of child tasks must be at least 2 (use --number <N>)");
    }

    let status: TaskStatus = args.status.unwrap_or(TaskStatusArg::Draft).into();
    let now = timeutil::now_utc_rfc3339()?;
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);

    // Load queue (source task must be in active queue)
    let queue_file = queue::load_queue(&resolved.queue_path)?;

    // Build split options
    let split_opts = queue::operations::SplitTaskOptions::new(
        &args.task_id,
        args.number,
        status,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
    )
    .with_title_prefix(args.title_prefix.as_deref())
    .with_distribute_plan(args.distribute_plan)
    .with_max_depth(max_depth);

    // Perform the split operation (dry-run check first)
    let (updated_source, child_tasks) =
        queue::operations::split_task(&mut queue_file.clone(), None, &split_opts)?;

    if args.dry_run {
        println!(
            "Dry run - would split task {} into {} child tasks:",
            args.task_id,
            child_tasks.len()
        );
        println!("\nOriginal task would be updated:");
        println!("  ID: {}", updated_source.id);
        println!("  Title: {}", updated_source.title);
        println!("  Status: {} (marked as split)", updated_source.status);
        if let Some(ref custom) = updated_source.custom_fields.get("split") {
            println!("  Custom field 'split': {}", custom);
        }
        println!("\nChild tasks to create:");
        for (i, child) in child_tasks.iter().enumerate() {
            println!("\n  {}. {}", i + 1, child.id);
            println!("     Title: {}", child.title);
            println!("     Status: {}", child.status);
            println!(
                "     Parent: {}",
                child.parent_id.as_deref().unwrap_or("none")
            );
            if !child.plan.is_empty() {
                println!("     Plan items: {}", child.plan.len());
            }
        }
        return Ok(());
    }

    // Acquire lock and perform actual split
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "task split", force)?;

    // Create undo snapshot before mutation
    crate::undo::create_undo_snapshot(
        resolved,
        &format!("task split {} into {} tasks", args.task_id, args.number),
    )?;

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;

    let (updated_source, child_tasks) =
        queue::operations::split_task(&mut queue_file, None, &split_opts)?;

    // Find and update the source task in the queue
    let source_index = queue_file
        .tasks
        .iter()
        .position(|t| t.id == args.task_id)
        .with_context(|| crate::error_messages::source_task_not_found(&args.task_id, false))?;
    queue_file.tasks[source_index] = updated_source;

    // Log and print output before moving child_tasks
    log::info!(
        "Split task {} into {} child tasks (status: {})",
        args.task_id,
        child_tasks.len(),
        status
    );
    println!(
        "Split task {} into {} child tasks:",
        args.task_id,
        child_tasks.len()
    );

    for child in &child_tasks {
        println!("  - Created {}", child.id);
    }

    // Insert child tasks at appropriate position (after the source task)
    let insert_at = source_index + 1;
    for (i, child) in child_tasks.into_iter().enumerate() {
        queue_file.tasks.insert(insert_at + i, child);
    }

    // Save queue
    queue::save_queue(&resolved.queue_path, &queue_file)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cli::task::args::TaskStatusArg;
    use crate::contracts::TaskStatus;

    #[test]
    fn task_status_arg_converts_to_task_status() {
        assert_eq!(TaskStatus::from(TaskStatusArg::Draft), TaskStatus::Draft);
        assert_eq!(TaskStatus::from(TaskStatusArg::Todo), TaskStatus::Todo);
        assert_eq!(TaskStatus::from(TaskStatusArg::Doing), TaskStatus::Doing);
        assert_eq!(TaskStatus::from(TaskStatusArg::Done), TaskStatus::Done);
        assert_eq!(
            TaskStatus::from(TaskStatusArg::Rejected),
            TaskStatus::Rejected
        );
    }
}
