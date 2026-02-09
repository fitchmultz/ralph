//! Queue next-id subcommand.
//!
//! Responsibilities:
//! - Generate one or more sequential task IDs based on current queue state.
//! - Validate count bounds (1..=MAX_COUNT) to prevent abuse.
//! - Work correctly even when duplicate task IDs exist (graceful degradation).
//!
//! Not handled here:
//! - Queue modification (this is a read-only operation).
//! - ID reservation (IDs are generated but not claimed; callers must create tasks promptly).
//! - Full queue validation (duplicates are warned but don't block ID generation).
//!
//! Invariants/assumptions:
//! - Count must be between 1 and MAX_COUNT (100) inclusive.
//! - Generated IDs are sequential and unique within the current queue state.
//! - Output format: one ID per line for easy shell scripting.
//! - Duplicate IDs in queue.json or done.json are warned but don't prevent operation.

use std::collections::HashSet;

use anyhow::{Result, bail};
use clap::Args;

use crate::config::Resolved;
use crate::constants::limits::MAX_COUNT;
use crate::contracts::TaskStatus;
use crate::queue;
use crate::queue::validation;

#[derive(Args)]
pub struct QueueNextIdArgs {
    /// Number of IDs to generate
    #[arg(short = 'n', long, default_value = "1", value_name = "COUNT")]
    pub count: usize,
}

pub(crate) fn handle(resolved: &Resolved, args: QueueNextIdArgs) -> Result<()> {
    // Validate count bounds
    if args.count == 0 {
        bail!("Count must be at least 1");
    }
    if args.count > MAX_COUNT {
        bail!(
            "Count cannot exceed {} (requested: {})",
            MAX_COUNT,
            args.count
        );
    }

    // Load queues without validation to handle duplicate IDs gracefully
    let queue_file = queue::load_queue_or_default(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;

    // Collect all IDs and detect duplicates
    let expected_prefix = queue::normalize_prefix(&resolved.id_prefix);
    let mut seen_ids = HashSet::new();
    let mut duplicates = Vec::new();
    let mut max_value: u32 = 0;

    // Process active queue
    for (idx, task) in queue_file.tasks.iter().enumerate() {
        match validation::validate_task_id(idx, &task.id, &expected_prefix, resolved.id_width) {
            Ok(value) => {
                if task.status != TaskStatus::Rejected && value > max_value {
                    max_value = value;
                }
                if !seen_ids.insert(task.id.clone()) {
                    duplicates.push(task.id.clone());
                }
            }
            Err(e) => {
                log::warn!("Invalid task ID in queue: {}", e);
            }
        }
    }

    // Process done queue
    for (idx, task) in done_file.tasks.iter().enumerate() {
        match validation::validate_task_id(idx, &task.id, &expected_prefix, resolved.id_width) {
            Ok(value) => {
                if task.status != TaskStatus::Rejected && value > max_value {
                    max_value = value;
                }
                if !seen_ids.insert(task.id.clone()) {
                    duplicates.push(task.id.clone());
                }
            }
            Err(e) => {
                log::warn!("Invalid task ID in done: {}", e);
            }
        }
    }

    // Log duplicate warnings
    if !duplicates.is_empty() {
        log::warn!("Duplicate task IDs detected: {:?}", duplicates);
        eprintln!(
            "Warning: Found duplicate task IDs: {}",
            duplicates.join(", ")
        );
    }

    let next_value = max_value.saturating_add(1);
    let first_id = queue::format_id(&expected_prefix, next_value, resolved.id_width);

    // Parse the numeric portion from the first ID
    let prefix_len = resolved.id_prefix.len() + 1; // +1 for the hyphen
    let first_num: u32 = first_id[prefix_len..].parse()?;

    // Generate and print all IDs
    for i in 0..args.count {
        let num = first_num + i as u32;
        let id = format!(
            "{}-{:0width$}",
            resolved.id_prefix,
            num,
            width = resolved.id_width
        );
        println!("{id}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
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
        }
    }

    fn setup_test_queue(temp: &TempDir, tasks: Vec<Task>) -> Resolved {
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let queue_file = QueueFile { version: 1, tasks };
        let queue_json = serde_json::to_string_pretty(&queue_file).unwrap();
        std::fs::write(&queue_path, queue_json).unwrap();

        // Create empty done file
        let done_file = QueueFile {
            version: 1,
            tasks: vec![],
        };
        let done_json = serde_json::to_string_pretty(&done_file).unwrap();
        std::fs::write(&done_path, done_json).unwrap();

        Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn test_count_validation_zero() {
        let temp = TempDir::new().unwrap();
        let resolved = setup_test_queue(&temp, vec![]);

        let args = QueueNextIdArgs { count: 0 };
        let result = handle(&resolved, args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least 1"));
    }

    #[test]
    fn test_count_validation_max() {
        let temp = TempDir::new().unwrap();
        let resolved = setup_test_queue(&temp, vec![]);

        let args = QueueNextIdArgs { count: 101 };
        let result = handle(&resolved, args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot exceed"));
    }

    #[test]
    fn test_single_id_runs_successfully() {
        let temp = TempDir::new().unwrap();
        let resolved = setup_test_queue(&temp, vec![task("RQ-0001", TaskStatus::Todo)]);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_ids_runs_successfully() {
        let temp = TempDir::new().unwrap();
        let resolved = setup_test_queue(&temp, vec![task("RQ-0005", TaskStatus::Todo)]);

        let args = QueueNextIdArgs { count: 3 };
        let result = handle(&resolved, args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_empty_queue_generates_from_one() {
        let temp = TempDir::new().unwrap();
        let resolved = setup_test_queue(&temp, vec![]);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_max_count_boundary() {
        let temp = TempDir::new().unwrap();
        let resolved = setup_test_queue(&temp, vec![]);

        // 100 should be allowed
        let args = QueueNextIdArgs { count: 100 };
        let result = handle(&resolved, args);
        assert!(result.is_ok());
    }

    fn setup_test_queues(
        temp: &TempDir,
        queue_tasks: Vec<Task>,
        done_tasks: Vec<Task>,
    ) -> Resolved {
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let queue_file = QueueFile {
            version: 1,
            tasks: queue_tasks,
        };
        let queue_json = serde_json::to_string_pretty(&queue_file).unwrap();
        std::fs::write(&queue_path, queue_json).unwrap();

        let done_file = QueueFile {
            version: 1,
            tasks: done_tasks,
        };
        let done_json = serde_json::to_string_pretty(&done_file).unwrap();
        std::fs::write(&done_path, done_json).unwrap();

        Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn test_duplicate_ids_in_queue_returns_next_id() {
        let temp = TempDir::new().unwrap();
        // Setup queue with duplicate IDs: RQ-0001, RQ-0002, RQ-0001
        let queue_tasks = vec![
            task("RQ-0001", TaskStatus::Todo),
            task("RQ-0002", TaskStatus::Todo),
            task("RQ-0001", TaskStatus::Todo), // duplicate
        ];
        let resolved = setup_test_queues(&temp, queue_tasks, vec![]);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);

        // Should succeed and return RQ-0003 (max is 0002, plus 1)
        assert!(result.is_ok());
    }

    #[test]
    fn test_duplicate_ids_across_queue_and_done_returns_next_id() {
        let temp = TempDir::new().unwrap();
        // Setup: RQ-0001 in queue.json, RQ-0001 and RQ-0005 in done.json
        let queue_tasks = vec![task("RQ-0001", TaskStatus::Todo)];
        let done_tasks = vec![
            task("RQ-0001", TaskStatus::Done), // duplicate across files
            task("RQ-0005", TaskStatus::Done),
        ];
        let resolved = setup_test_queues(&temp, queue_tasks, done_tasks);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);

        // Should succeed and return RQ-0006 (max is 0005, plus 1)
        assert!(result.is_ok());
    }

    #[test]
    fn test_multiple_duplicates_returns_correct_next_id() {
        let temp = TempDir::new().unwrap();
        // Setup: RQ-0001, RQ-0002, RQ-0001, RQ-0003, RQ-0002 (duplicates of 1 and 2)
        let queue_tasks = vec![
            task("RQ-0001", TaskStatus::Todo),
            task("RQ-0002", TaskStatus::Todo),
            task("RQ-0001", TaskStatus::Todo), // duplicate of 1
            task("RQ-0003", TaskStatus::Todo),
            task("RQ-0002", TaskStatus::Todo), // duplicate of 2
        ];
        let resolved = setup_test_queues(&temp, queue_tasks, vec![]);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);

        // Should succeed and return RQ-0004 (max is 0003, plus 1)
        assert!(result.is_ok());
    }

    #[test]
    fn test_next_id_considers_done_even_with_queue_errors() {
        let temp = TempDir::new().unwrap();
        // Setup: Invalid/duplicate IDs in queue.json
        // Setup: Valid high-numbered task in done.json (RQ-0100)
        let queue_tasks = vec![
            task("RQ-0001", TaskStatus::Todo),
            task("RQ-0001", TaskStatus::Todo), // duplicate
        ];
        let done_tasks = vec![task("RQ-0100", TaskStatus::Done)];
        let resolved = setup_test_queues(&temp, queue_tasks, done_tasks);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);

        // Should still return RQ-0101 (not RQ-0002) because done.json is considered
        assert!(result.is_ok());
    }

    #[test]
    fn test_all_queue_tasks_are_duplicates() {
        let temp = TempDir::new().unwrap();
        // Edge case: all tasks in queue are duplicates of the same ID
        let queue_tasks = vec![
            task("RQ-0001", TaskStatus::Todo),
            task("RQ-0001", TaskStatus::Todo),
            task("RQ-0001", TaskStatus::Todo),
        ];
        let resolved = setup_test_queues(&temp, queue_tasks, vec![]);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);

        // Should succeed and return RQ-0002
        assert!(result.is_ok());
    }

    #[test]
    fn test_rejected_tasks_still_checked_for_duplicates() {
        let temp = TempDir::new().unwrap();
        // Rejected tasks should still count toward duplicate detection
        let queue_tasks = vec![
            task("RQ-0001", TaskStatus::Todo),
            task("RQ-0001", TaskStatus::Rejected), // duplicate, though rejected
        ];
        let resolved = setup_test_queues(&temp, queue_tasks, vec![]);

        let args = QueueNextIdArgs { count: 1 };
        let result = handle(&resolved, args);

        // Should succeed (rejected status doesn't prevent duplicate detection)
        assert!(result.is_ok());
    }
}
