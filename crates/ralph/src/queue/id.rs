//! Task ID generation and formatting utilities.
//!
//! Purpose:
//! - Task ID generation and formatting utilities.
//!
//! Responsibilities:
//! - Generate next available task ID across active and done queues.
//! - Normalize ID prefixes and format IDs with proper zero-padding.
//!
//! Not handled here:
//! - ID validation (see `queue::validation`).
//! - Queue persistence or file operations.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task IDs follow the pattern: PREFIX-XXXX where X is a digit.
//! - Rejected tasks are skipped when determining next ID.

use crate::contracts::{QueueFile, TaskStatus};
use crate::queue::validation::{self, log_warnings, validate_queue_set};
use anyhow::Result;

pub fn next_id_across(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<String> {
    let warnings = validate_queue_set(active, done, id_prefix, id_width, max_dependency_depth)?;
    log_warnings(&warnings);
    let expected_prefix = normalize_prefix(id_prefix);

    let mut max_value: u32 = 0;
    for (idx, task) in active.tasks.iter().enumerate() {
        let value = validation::validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
        if task.status == TaskStatus::Rejected {
            continue;
        }
        if value > max_value {
            max_value = value;
        }
    }
    if let Some(done) = done {
        for (idx, task) in done.tasks.iter().enumerate() {
            let value = validation::validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
            if task.status == TaskStatus::Rejected {
                continue;
            }
            if value > max_value {
                max_value = value;
            }
        }
    }

    let next_value = max_value.saturating_add(1);
    Ok(format_id(&expected_prefix, next_value, id_width))
}

pub fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().to_uppercase()
}

pub fn format_id(prefix: &str, number: u32, width: usize) -> String {
    format!("{}-{:0width$}", prefix, number, width = width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::collections::HashMap;

    fn task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec!["code".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
            estimated_minutes: None,
            actual_minutes: None,
        }
    }

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags,
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
            estimated_minutes: None,
            actual_minutes: None,
        }
    }

    #[test]
    fn next_id_across_includes_done() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0002")],
        };
        let mut done_task = task_with("RQ-0009", TaskStatus::Done, vec!["tag".to_string()]);
        done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let done = QueueFile {
            version: 1,
            tasks: vec![done_task],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4, 10)?;
        assert_eq!(next, "RQ-0010");
        Ok(())
    }

    #[test]
    fn next_id_across_ignores_rejected() -> Result<()> {
        let mut t_rejected = task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]);
        t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
                t_rejected,
            ],
        };
        let next = next_id_across(&active, None, "RQ", 4, 10)?;
        assert_eq!(next, "RQ-0002");
        Ok(())
    }

    #[test]
    fn next_id_across_includes_done_non_rejected() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![task_with(
                "RQ-0001",
                TaskStatus::Todo,
                vec!["tag".to_string()],
            )],
        };
        let mut t_done = task_with("RQ-0005", TaskStatus::Done, vec!["tag".to_string()]);
        t_done.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let mut t_rejected = task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]);
        t_rejected.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let done = QueueFile {
            version: 1,
            tasks: vec![t_done, t_rejected],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4, 10)?;
        assert_eq!(next, "RQ-0006");
        Ok(())
    }
}
