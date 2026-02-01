//! Queue next-id subcommand.
//!
//! Responsibilities:
//! - Generate one or more sequential task IDs based on current queue state.
//! - Validate count bounds (1..=MAX_COUNT) to prevent abuse.
//!
//! Not handled here:
//! - Queue modification (this is a read-only operation).
//! - ID reservation (IDs are generated but not claimed; callers must create tasks promptly).
//!
//! Invariants/assumptions:
//! - Count must be between 1 and MAX_COUNT (100) inclusive.
//! - Generated IDs are sequential and unique within the current queue state.
//! - Output format: one ID per line for easy shell scripting.

use anyhow::{Result, bail};
use clap::Args;

use crate::cli::load_and_validate_queues;
use crate::config::Resolved;
use crate::constants::limits::MAX_COUNT;
use crate::queue;

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

    let (queue_file, done_file) = load_and_validate_queues(resolved, true)?;
    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    // Get the first ID using next_id_across
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let first_id = queue::next_id_across(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

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
}
