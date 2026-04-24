//! Handler for `ralph queue tree` subcommand.
//!
//! Purpose:
//! - Handler for `ralph queue tree` subcommand.
//!
//! Responsibilities:
//! - Render an ASCII tree of task hierarchy based on parent_id.
//! - Support filtering by root task, max depth, and done inclusion.
//! - Show orphan markers for tasks with missing parents.
//!
//! Not handled here:
//! - Queue mutation (this is a read-only command).
//! - Dependency graph rendering (see `graph.rs` for depends_on).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Output is deterministic (stable ordering).
//! - Cycles are detected and marked but don't cause infinite recursion.

use anyhow::{Result, bail};
use clap::Args;

use crate::cli::load_and_validate_queues_read_only;
use crate::config::Resolved;
use crate::queue::hierarchy::{HierarchyIndex, TaskSource, detect_parent_cycles, render_tree};

/// Arguments for `ralph queue tree`.
#[derive(Args)]
#[command(
    about = "Render a parent/child hierarchy tree (based on parent_id)",
    after_long_help = "Examples:\n  ralph queue tree\n  ralph queue tree --include-done\n  ralph queue tree --root RQ-0001\n  ralph queue tree --max-depth 25"
)]
pub struct QueueTreeArgs {
    #[arg(long, value_name = "TASK_ID")]
    pub root: Option<String>,

    #[arg(long)]
    pub include_done: bool,

    #[arg(long, default_value = "20")]
    pub max_depth: usize,
}

/// Handle the `queue tree` command.
pub fn handle(resolved: &Resolved, args: QueueTreeArgs) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues_read_only(resolved, args.include_done)?;

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Build hierarchy index
    let idx = HierarchyIndex::build(&queue_file, done_ref);

    // Determine roots to render
    let roots: Vec<String> = if let Some(ref root_id) = args.root {
        // Validate root exists
        if !idx.contains(root_id) {
            if !args.include_done {
                bail!(
                    "{}",
                    crate::error_messages::root_task_not_found(root_id, false)
                );
            }
            bail!(
                "{}",
                crate::error_messages::root_task_not_found(root_id, true)
            );
        }
        vec![root_id.clone()]
    } else {
        // Compute all roots deterministically
        idx.roots()
            .iter()
            .filter(|r| args.include_done || matches!(r.source, TaskSource::Active))
            .map(|r| r.task.id.clone())
            .collect()
    };

    let roots = if roots.is_empty() {
        // This happens when every task has a non-empty parent_id that points to an existing task.
        // In a finite parent-pointer graph, that implies one or more cycles; render cycle entry points.
        let mut all_tasks: Vec<&crate::contracts::Task> = queue_file.tasks.iter().collect();
        if args.include_done
            && let Some(done_file) = done_ref
        {
            all_tasks.extend(done_file.tasks.iter());
        }

        let cycles = detect_parent_cycles(&all_tasks);
        if cycles.is_empty() {
            println!("No tasks with hierarchy (parent_id) found.");
            return Ok(());
        }

        let mut cycle_roots: Vec<_> = cycles
            .iter()
            .filter_map(|cycle| cycle.first())
            .filter(|id| idx.contains(id))
            .filter_map(|id| idx.get(id))
            .collect();
        cycle_roots.sort_by_key(|r| r.order);
        let roots: Vec<String> = cycle_roots.iter().map(|r| r.task.id.clone()).collect();

        if roots.is_empty() {
            println!("No tasks with hierarchy (parent_id) found.");
            return Ok(());
        }

        println!("[Note: no root tasks found; rendering parent_id cycles]");
        roots
    } else {
        roots
    };

    // Render the tree
    let root_refs: Vec<&str> = roots.iter().map(|s| s.as_str()).collect();
    let output = render_tree(
        &idx,
        &root_refs,
        args.max_depth,
        args.include_done,
        |task, depth, is_cycle, orphan_parent| {
            let indent = "  ".repeat(depth);
            let prefix = if depth == 0 { "" } else { "└─ " };
            let base = format!("{}{}{}", indent, prefix, task.id);

            if is_cycle {
                return format!(
                    "{}: {} [{}] (cycle)",
                    base,
                    task.title,
                    task.status.as_str()
                );
            }

            if let Some(parent) = orphan_parent {
                return format!(
                    "{}: {} [{}] (orphan: missing parent {})",
                    base,
                    task.title,
                    task.status.as_str(),
                    parent
                );
            }

            format!("{}: {} [{}]", base, task.title, task.status.as_str())
        },
    );

    if output.trim().is_empty() {
        println!("No tasks with hierarchy (parent_id) found.");
    } else {
        println!("{}", output.trim());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_args_defaults() {
        // Simple test that the struct can be created
        let args = QueueTreeArgs {
            root: None,
            include_done: false,
            max_depth: 20,
        };
        assert_eq!(args.root, None);
        assert_eq!(args.max_depth, 20);
        assert!(!args.include_done);
    }

    #[test]
    fn tree_args_with_root() {
        let args = QueueTreeArgs {
            root: Some("RQ-0001".to_string()),
            include_done: true,
            max_depth: 10,
        };
        assert_eq!(args.root, Some("RQ-0001".to_string()));
        assert_eq!(args.max_depth, 10);
        assert!(args.include_done);
    }
}
