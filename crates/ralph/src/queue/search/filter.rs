//! Task filtering by status, tag, and scope.
//!
//! Purpose:
//! - Task filtering by status, tag, and scope.
//!
//! Responsibilities:
//! - Filter tasks by status, tags, and scope with AND across categories
//! - Support limiting results to N matches
//!
//! Not handled here:
//! - Text search (see substring.rs and fuzzy.rs)
//! - Sorting or ordering beyond input order
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Empty filter lists mean "no filtering" for that category
//! - Within-category matching is OR (any status, any tag, any scope token)
//! - Tag matching is case-insensitive exact match after trim/lowercase
//! - Scope matching is case-insensitive substring match
//! - Limit stops after N matches (preserves input order)

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue::search::normalize::normalize;
use std::collections::HashSet;

pub fn filter_tasks<'a>(
    queue: &'a QueueFile,
    statuses: &[TaskStatus],
    tags: &[String],
    scopes: &[String],
    limit: Option<usize>,
) -> Vec<&'a Task> {
    let status_filter: HashSet<TaskStatus> = statuses.iter().copied().collect();
    let tag_filter: HashSet<String> = tags
        .iter()
        .map(|tag| normalize(tag))
        .filter(|tag| !tag.is_empty())
        .collect();
    let scope_filter: Vec<String> = scopes
        .iter()
        .map(|scope| normalize(scope))
        .filter(|scope| !scope.is_empty())
        .collect();

    let has_status_filter = !status_filter.is_empty();
    let has_tag_filter = !tag_filter.is_empty();
    let has_scope_filter = !scope_filter.is_empty();

    let mut out = Vec::new();
    for task in &queue.tasks {
        if has_status_filter && !status_filter.contains(&task.status) {
            continue;
        }
        if has_tag_filter
            && !task
                .tags
                .iter()
                .any(|tag| tag_filter.contains(&normalize(tag)))
        {
            continue;
        }
        if has_scope_filter
            && !task.scope.iter().any(|scope| {
                let hay = normalize(scope);
                scope_filter.iter().any(|needle| hay.contains(needle))
            })
        {
            continue;
        }

        out.push(task);
        if let Some(limit) = limit
            && out.len() >= limit
        {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, TaskStatus};
    use crate::queue::search::test_support::{task_with_scope, task_with_tags_scope_status};

    #[test]
    fn filter_tasks_with_scope_filter() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
                task_with_scope("RQ-0003", vec!["crates/auth".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &["crates/ralph".to_string()], None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
    }

    #[test]
    fn filter_tasks_scope_filter_case_insensitive() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["CRATES/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &["crates/ralph".to_string()], None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
    }

    #[test]
    fn filter_tasks_scope_filter_substring() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph/src/cli".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
                task_with_scope("RQ-0003", vec!["crates/auth".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &["crates/ralph".to_string()], None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "RQ-0001");
    }

    #[test]
    fn filter_tasks_with_multiple_scopes_or_logic() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs".to_string()]),
                task_with_scope("RQ-0003", vec!["crates/auth".to_string()]),
            ],
        };

        let results = filter_tasks(
            &queue,
            &[],
            &[],
            &["crates/ralph".to_string(), "docs".to_string()],
            None,
        );
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|t| t.id == "RQ-0001"));
        assert!(results.iter().any(|t| t.id == "RQ-0002"));
    }

    #[test]
    fn filter_tasks_with_no_scope_filter() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_scope("RQ-0001", vec!["crates/ralph".to_string()]),
                task_with_scope("RQ-0002", vec!["docs/cli".to_string()]),
            ],
        };

        let results = filter_tasks(&queue, &[], &[], &[], None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn filter_tasks_combined_filters() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status(
                    "RQ-0001",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0002",
                    vec!["docs".to_string()],
                    vec!["docs".to_string()],
                    TaskStatus::Done,
                ),
                task_with_tags_scope_status(
                    "RQ-0003",
                    vec!["rust".to_string()],
                    vec!["crates".to_string()],
                    TaskStatus::Doing,
                ),
                task_with_tags_scope_status(
                    "RQ-0004",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
            ],
        };

        let results = filter_tasks(
            &queue,
            &[TaskStatus::Todo],
            &["rust".to_string()],
            &["crates/ralph".to_string()],
            None,
        );
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|t| t.id == "RQ-0001"));
        assert!(results.iter().any(|t| t.id == "RQ-0004"));
    }

    #[test]
    fn filter_tasks_status_only() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status("RQ-0001", vec![], vec![], TaskStatus::Todo),
                task_with_tags_scope_status("RQ-0002", vec![], vec![], TaskStatus::Doing),
                task_with_tags_scope_status("RQ-0003", vec![], vec![], TaskStatus::Todo),
            ],
        };

        let results = filter_tasks(&queue, &[TaskStatus::Todo], &[], &[], None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|t| t.status == TaskStatus::Todo));
    }

    #[test]
    fn filter_tasks_tag_only() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status(
                    "RQ-0001",
                    vec!["rust".to_string()],
                    vec![],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0002",
                    vec!["docs".to_string()],
                    vec![],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0003",
                    vec!["RUST".to_string()],
                    vec![],
                    TaskStatus::Doing,
                ),
            ],
        };

        let results = filter_tasks(&queue, &[], &["rust".to_string()], &[], None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|t| t.id == "RQ-0001"));
        assert!(results.iter().any(|t| t.id == "RQ-0003"));
    }

    #[test]
    fn filter_tasks_with_limit() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![
                task_with_tags_scope_status(
                    "RQ-0001",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0002",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
                task_with_tags_scope_status(
                    "RQ-0003",
                    vec!["rust".to_string()],
                    vec!["crates/ralph".to_string()],
                    TaskStatus::Todo,
                ),
            ],
        };

        let results = filter_tasks(
            &queue,
            &[TaskStatus::Todo],
            &["rust".to_string()],
            &["crates/ralph".to_string()],
            Some(2),
        );
        assert_eq!(results.len(), 2);
    }
}
