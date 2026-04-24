//! Handler for `ralph task parent` subcommand.
//!
//! Purpose:
//! - Handler for `ralph task parent` subcommand.
//!
//! Responsibilities:
//! - Show a task's parent (based on parent_id).
//! - Display sibling count (children of the same parent).
//! - Provide multiple output formats (compact, long, json).
//!
//! Not handled here:
//! - Queue mutation (this is a read-only command).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task existence is validated before processing.
//! - Missing parent references are handled gracefully.

use anyhow::Result;
use serde_json::json;

use crate::cli::load_and_validate_queues_read_only;
use crate::cli::task::args::{TaskParentArgs, TaskRelationFormat};
use crate::config::Resolved;
use crate::queue::hierarchy::HierarchyIndex;

/// Handle the `task parent` command.
pub fn handle(args: &TaskParentArgs, resolved: &Resolved) -> Result<()> {
    let (queue_file, done_file) = load_and_validate_queues_read_only(resolved, args.include_done)?;

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Build hierarchy index
    let idx = HierarchyIndex::build(&queue_file, done_ref);

    // Find the task
    let task_id = args.task_id.trim();
    let task_ref = idx.get(task_id).ok_or_else(|| {
        if !args.include_done {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_with_include_done_hint(task_id)
            )
        } else {
            anyhow::anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue_or_done(task_id)
            )
        }
    })?;

    let task = task_ref.task;

    // Get parent info
    let parent_id_opt = task.parent_id.as_deref().and_then(|p| {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    match parent_id_opt {
        None => {
            // No parent
            match args.format {
                TaskRelationFormat::Compact | TaskRelationFormat::Long => {
                    println!("Task {} has no parent.", task_id);
                }
                TaskRelationFormat::Json => {
                    let output = json!({
                        "task_id": task_id,
                        "parent": null,
                        "siblings": 0
                    });
                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                }
            }
        }
        Some(parent_id) => {
            // Has parent_id - check if parent exists
            let parent_ref = idx.get(parent_id);

            match parent_ref {
                None => {
                    // Parent reference but parent not found
                    let suggestion = if !args.include_done {
                        " (use --include-done to search done archive)"
                    } else {
                        ""
                    };

                    match args.format {
                        TaskRelationFormat::Compact | TaskRelationFormat::Long => {
                            println!(
                                "Task {} references parent {} which was not found{}.",
                                task_id, parent_id, suggestion
                            );
                        }
                        TaskRelationFormat::Json => {
                            let output = json!({
                                "task_id": task_id,
                                "parent": parent_id,
                                "parent_found": false,
                                "siblings": 0
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        }
                    }
                }
                Some(parent_task_ref) => {
                    // Parent found - show it
                    let siblings = idx.children_of(parent_id);
                    let sibling_count = siblings.len().saturating_sub(1); // Exclude self

                    match args.format {
                        TaskRelationFormat::Compact => {
                            println!(
                                "{}: {} [{}]",
                                parent_task_ref.task.id,
                                parent_task_ref.task.title,
                                parent_task_ref.task.status.as_str()
                            );
                            println!("Siblings: {}", sibling_count);
                        }
                        TaskRelationFormat::Long => {
                            println!(
                                "{}: {} [{}] priority={}",
                                parent_task_ref.task.id,
                                parent_task_ref.task.title,
                                parent_task_ref.task.status.as_str(),
                                parent_task_ref.task.priority.as_str()
                            );
                            println!("Siblings: {}", sibling_count);
                        }
                        TaskRelationFormat::Json => {
                            let output = json!({
                                "task_id": task_id,
                                "parent": parent_task_ref.task,
                                "parent_found": true,
                                "siblings": sibling_count
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};

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
    fn parent_detection_no_parent() {
        let active = QueueFile {
            version: 1,
            tasks: vec![make_task("RQ-0001", None)],
        };
        let idx = HierarchyIndex::build(&active, None);

        let task = idx.get("RQ-0001").unwrap();
        assert!(task.task.parent_id.is_none());
    }

    #[test]
    fn parent_detection_with_parent() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                make_task("RQ-0001", None),
                make_task("RQ-0002", Some("RQ-0001")),
            ],
        };
        let idx = HierarchyIndex::build(&active, None);

        let task = idx.get("RQ-0002").unwrap();
        assert_eq!(task.task.parent_id.as_deref(), Some("RQ-0001"));

        let children = idx.children_of("RQ-0001");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].task.id, "RQ-0002");
    }

    #[test]
    fn sibling_count_calculation() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                make_task("RQ-0001", None),
                make_task("RQ-0002", Some("RQ-0001")),
                make_task("RQ-0003", Some("RQ-0001")),
                make_task("RQ-0004", Some("RQ-0001")),
            ],
        };
        let idx = HierarchyIndex::build(&active, None);

        let children = idx.children_of("RQ-0001");
        assert_eq!(children.len(), 3);

        // Each child has 2 siblings
        for _child in children {
            let sibling_count = children.len().saturating_sub(1);
            assert_eq!(sibling_count, 2);
        }
    }
}
