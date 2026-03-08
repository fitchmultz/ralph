//! Task queue persistence, validation, and pruning.
//!
//! Responsibilities:
//! - Load, save, and validate queue files in JSON format.
//! - Provide operations for moving completed tasks and pruning history.
//! - Own queue-level helpers such as ID generation and validation.
//!
//! Not handled here:
//! - Directory lock acquisition (see `crate::lock`).
//! - CLI parsing or user interaction.
//! - Runner integration or external command execution.
//!
//! Invariants/assumptions:
//! - Queue files conform to the schema in `crate::contracts`.
//! - Callers hold locks when mutating queue state on disk.

// Existing submodules (unchanged)
pub mod graph;
pub mod hierarchy;
pub mod operations;
pub mod prune;
pub mod repair;
pub mod search;
pub mod size_check;
pub mod validation;

// New split modules
mod backup;
mod id;
mod json_repair;
mod loader;
mod save;

// Re-exports from existing submodules
pub use graph::*;
pub use operations::*;
pub use prune::{PruneOptions, PruneReport, prune_done_tasks};
pub use repair::*;
pub use search::{
    SearchOptions, filter_tasks, fuzzy_search_tasks, search_tasks, search_tasks_with_options,
};
pub use size_check::{
    SizeCheckResult, check_queue_size, count_threshold_or_default, print_size_warning_if_needed,
    size_threshold_or_default,
};
pub use validation::{ValidationWarning, log_warnings, validate_queue, validate_queue_set};

// Re-exports from new modules.
pub use backup::backup_queue;
pub use id::next_id_across;
pub use id::{format_id, normalize_prefix};
pub use json_repair::attempt_json_repair;
pub use loader::{
    load_and_validate_queues, load_queue, load_queue_or_default, load_queue_with_repair,
    load_queue_with_repair_and_validate, repair_and_validate_queues,
};
pub use save::save_queue;

use crate::lock;
use anyhow::Result;
use std::path::Path;

pub fn acquire_queue_lock(repo_root: &Path, label: &str, force: bool) -> Result<lock::DirLock> {
    let lock_dir = lock::queue_lock_dir(repo_root);
    lock::acquire_dir_lock(&lock_dir, label, force)
}

// Tests that exercise the facade re-exports
#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
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

    #[test]
    fn task_defaults_to_medium_priority() {
        use crate::contracts::TaskPriority;
        let task = task("RQ-0001");
        assert_eq!(task.priority, TaskPriority::Medium);
    }

    #[test]
    fn priority_ord_implements_correct_ordering() {
        use crate::contracts::TaskPriority;
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Medium);
        assert!(TaskPriority::Medium > TaskPriority::Low);
    }

    #[test]
    fn attempt_json_repair_fixes_trailing_comma_in_array() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "tags": ["a", "b",]}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"tags\": [\"a\", \"b\"]"));
        assert!(!repaired.contains("\"b\","));
    }

    #[test]
    fn attempt_json_repair_fixes_trailing_comma_in_object() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Test",}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"title\": \"Test\"}"));
        assert!(!repaired.contains("\"Test\","));
    }

    #[test]
    fn attempt_json_repair_returns_none_for_valid_json() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Test"}]}"#;
        assert!(attempt_json_repair(input).is_none());
    }
}
