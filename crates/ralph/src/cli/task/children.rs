//! Handler for `ralph task children` subcommand.
//!
//! Responsibilities:
//! - List child tasks where parent_id matches the given task ID.
//! - Support recursive listing with tree rendering.
//! - Provide multiple output formats (compact, long, json).
//!
//! Not handled here:
//! - Queue mutation (this is a read-only command).
//!
//! Invariants/assumptions:
//! - Task existence is validated before processing.
//! - Output is deterministic (stable ordering).

use anyhow::{Context, Result, bail};

use crate::cli::load_and_validate_queues;
use crate::cli::task::args::{TaskChildrenArgs, TaskRelationFormat};
use crate::config::Resolved;
use crate::contracts::Task;
use crate::outpututil;
use crate::queue::hierarchy::{HierarchyIndex, render_tree};
use std::collections::HashSet;

/// Handle the `task children` command.
pub fn handle(args: &TaskChildrenArgs, resolved: &Resolved) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues(resolved, args.include_done)?;

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Build hierarchy index
    let idx = HierarchyIndex::build(&queue_file, done_ref);

    // Validate task exists
    let task_id = args.task_id.trim();
    if !idx.contains(task_id) {
        if !args.include_done {
            bail!(
                "Task '{}' not found in active queue. Use --include-done to search done archive.",
                task_id
            );
        }
        bail!("Task '{}' not found in queue or done archive.", task_id);
    }

    // Collect output
    let output = if args.recursive {
        render_children_recursive(&idx, task_id, args.include_done, args.format)?
    } else {
        render_children_direct(&idx, task_id, args.format)?
    };

    println!("{}", output);
    Ok(())
}

/// Render direct children only (non-recursive).
fn render_children_direct(
    idx: &HierarchyIndex<'_>,
    task_id: &str,
    format: TaskRelationFormat,
) -> Result<String> {
    let children = idx.children_of(task_id);

    match format {
        TaskRelationFormat::Compact => {
            if children.is_empty() {
                return Ok("No children.".to_string());
            }
            let lines: Vec<String> = children
                .iter()
                .map(|c| outpututil::format_task_compact(c.task))
                .collect();
            Ok(lines.join("\n"))
        }
        TaskRelationFormat::Long => {
            if children.is_empty() {
                return Ok("No children.".to_string());
            }
            let lines: Vec<String> = children
                .iter()
                .map(|c| format_task_detailed(c.task))
                .collect();
            Ok(lines.join("\n"))
        }
        TaskRelationFormat::Json => {
            let tasks: Vec<&Task> = children.iter().map(|c| c.task).collect();
            serde_json::to_string_pretty(&tasks).context("Failed to serialize children to JSON")
        }
    }
}

/// Render recursive children tree.
fn render_children_recursive(
    idx: &HierarchyIndex<'_>,
    task_id: &str,
    include_done: bool,
    format: TaskRelationFormat,
) -> Result<String> {
    let children = idx.children_of(task_id);

    if children.is_empty() {
        return Ok("No children.".to_string());
    }

    match format {
        TaskRelationFormat::Compact | TaskRelationFormat::Long => {
            let use_detailed = matches!(format, TaskRelationFormat::Long);
            let output = render_tree(
                idx,
                &[task_id],
                50, // max_depth
                include_done,
                |task, depth, is_cycle, orphan_parent| {
                    let indent = "  ".repeat(depth);
                    let prefix = if depth == 0 { "" } else { "└─ " };
                    let base = format!("{}{}{}", indent, prefix, task.id);

                    if is_cycle {
                        return format!("{} (cycle)", base);
                    }

                    if let Some(parent) = orphan_parent {
                        return format!("{} (orphan: missing parent {})", base, parent);
                    }

                    if use_detailed {
                        format!("{}: {} [{}]", base, task.title, task.status.as_str())
                    } else {
                        format!("{}: {}", base, task.title)
                    }
                },
            );

            // Remove the root task line (first line) since we only want children
            let lines: Vec<&str> = output.lines().collect();
            if lines.len() <= 1 {
                Ok("No children.".to_string())
            } else {
                Ok(lines[1..].join("\n"))
            }
        }
        TaskRelationFormat::Json => {
            // For JSON, return a structured object with depth metadata
            let mut result = Vec::new();
            let mut path: HashSet<String> = HashSet::new();
            collect_children_recursive(idx, task_id, 0, 50, &mut path, &mut result);
            serde_json::to_string_pretty(&result)
                .context("Failed to serialize recursive children to JSON")
        }
    }
}

#[derive(serde::Serialize)]
struct ChildWithDepth<'a> {
    depth: usize,
    task: &'a Task,
    cycle: bool,
}

fn collect_children_recursive<'a>(
    idx: &HierarchyIndex<'a>,
    parent_id: &str,
    depth: usize,
    max_depth: usize,
    path: &mut HashSet<String>,
    result: &mut Vec<ChildWithDepth<'a>>,
) {
    if depth > max_depth {
        return;
    }

    let children = idx.children_of(parent_id);
    for child in children {
        let child_id = child.task.id.trim();

        if path.contains(child_id) {
            result.push(ChildWithDepth {
                depth,
                task: child.task,
                cycle: true,
            });
            continue;
        }

        path.insert(child_id.to_string());
        result.push(ChildWithDepth {
            depth,
            task: child.task,
            cycle: false,
        });
        collect_children_recursive(idx, child_id, depth + 1, max_depth, path, result);
        path.remove(child_id);
    }
}

/// Format a task in detailed/long format.
fn format_task_detailed(task: &Task) -> String {
    format!(
        "{}: {} [{}] priority={}",
        task.id,
        task.title,
        task.status.as_str(),
        task.priority.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, TaskStatus};

    fn make_task(id: &str, parent_id: Option<&str>) -> Task {
        Task {
            id: id.to_string(),
            title: format!("Task {}", id),
            description: None,
            status: TaskStatus::Todo,
            parent_id: parent_id.map(|s| s.to_string()),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn render_children_direct_empty() {
        let active = QueueFile {
            version: 1,
            tasks: vec![make_task("RQ-0001", None)],
        };
        let idx = HierarchyIndex::build(&active, None);

        let output = render_children_direct(&idx, "RQ-0001", TaskRelationFormat::Compact).unwrap();
        assert_eq!(output, "No children.");
    }

    #[test]
    fn render_children_direct_compact() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                make_task("RQ-0001", None),
                make_task("RQ-0002", Some("RQ-0001")),
            ],
        };
        let idx = HierarchyIndex::build(&active, None);

        let output = render_children_direct(&idx, "RQ-0001", TaskRelationFormat::Compact).unwrap();
        assert!(output.contains("RQ-0002"));
        assert!(output.contains("Task RQ-0002"));
    }

    #[test]
    fn render_children_direct_json() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                make_task("RQ-0001", None),
                make_task("RQ-0002", Some("RQ-0001")),
            ],
        };
        let idx = HierarchyIndex::build(&active, None);

        let output = render_children_direct(&idx, "RQ-0001", TaskRelationFormat::Json).unwrap();
        assert!(output.contains("RQ-0002"));
        assert!(output.contains("[")); // JSON array
    }

    #[test]
    fn render_children_recursive_json_is_cycle_safe() {
        // Cycle: 0001 <-> 0002
        let active = QueueFile {
            version: 1,
            tasks: vec![
                make_task("RQ-0001", Some("RQ-0002")),
                make_task("RQ-0002", Some("RQ-0001")),
            ],
        };
        let idx = HierarchyIndex::build(&active, None);

        let output =
            render_children_recursive(&idx, "RQ-0001", false, TaskRelationFormat::Json).unwrap();
        assert!(output.contains("\"cycle\": true"), "output={output}");
    }
}
