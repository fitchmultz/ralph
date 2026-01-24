//! Task queue persistence, validation, and pruning.
//!
//! This module handles loading, saving, and validating task queues stored
//! as JSON files (.ralph/queue.json for active tasks, .ralph/done.json
//! for completed tasks). It provides operations for moving completed tasks,
//! updating task status, repairing queue data, and pruning old tasks from
//! the done archive.

use crate::config::Resolved;
use crate::contracts::{QueueFile, TaskStatus};
use crate::fsutil;
use anyhow::{Context, Result};
use std::path::Path;

pub mod operations;
pub mod prune;
pub mod repair;
pub mod search;
pub mod validation;

pub use operations::*;
pub use prune::{prune_done_tasks, PruneOptions, PruneReport};
pub use repair::*;
pub use search::{filter_tasks, search_tasks, SearchOptions};
pub use validation::{validate_queue, validate_queue_set};

// Pruning types live in `queue::prune` (re-exported from this module).

pub fn acquire_queue_lock(repo_root: &Path, label: &str, force: bool) -> Result<fsutil::DirLock> {
    let lock_dir = fsutil::queue_lock_dir(repo_root);
    fsutil::acquire_dir_lock(&lock_dir, label, force)
}

pub fn load_queue_or_default(path: &Path) -> Result<QueueFile> {
    if !path.exists() {
        return Ok(QueueFile::default());
    }
    load_queue(path)
}

pub fn load_queue(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;
    let queue = serde_json::from_str::<QueueFile>(&raw)
        .with_context(|| format!("parse queue {} as JSON", path.display()))?;
    Ok(queue)
}

/// Load the active queue and optionally the done queue, validating both.
pub fn load_and_validate_queues(
    resolved: &Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    let queue_file = load_queue(&resolved.queue_path)?;

    let done_file = if include_done {
        Some(load_queue_or_default(&resolved.done_path)?)
    } else {
        None
    };

    let done_ref = done_file
        .as_ref()
        .filter(|d| !d.tasks.is_empty() || resolved.done_path.exists());

    if let Some(d) = done_ref {
        validate_queue_set(&queue_file, Some(d), &resolved.id_prefix, resolved.id_width)?;
    } else {
        validate_queue(&queue_file, &resolved.id_prefix, resolved.id_width)?;
    }

    Ok((queue_file, done_file))
}

pub fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
    let rendered = serde_json::to_string_pretty(queue).context("serialize queue JSON")?;
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue JSON {}", path.display()))?;
    Ok(())
}

pub fn next_id_across(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<String> {
    validate_queue_set(active, done, id_prefix, id_width)?;
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

pub(crate) fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().to_uppercase()
}

pub(crate) fn format_id(prefix: &str, number: u32, width: usize) -> String {
    format!("{}-{:0width$}", prefix, number, width = width)
}

// Pruning implementation moved to `queue::prune` (re-exported from this module).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn task(id: &str) -> Task {
        task_with(id, TaskStatus::Todo, vec!["code".to_string()])
    }

    // Pruning test helpers moved to `queue/prune.rs`.

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
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
            depends_on: vec![],
            custom_fields: HashMap::new(),
        }
    }

    #[test]
    fn next_id_across_includes_done() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0002")],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0009")],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4)?;
        assert_eq!(next, "RQ-0010");
        Ok(())
    }

    #[test]
    fn load_and_validate_queues_allows_missing_done_file() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;
        let done_path = ralph_dir.join("done.json");

        let resolved = Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let (queue, done) = load_and_validate_queues(&resolved, true)?;
        assert_eq!(queue.tasks.len(), 1);
        assert!(done.is_some());
        assert!(done.unwrap().tasks.is_empty());
        Ok(())
    }

    #[test]
    fn load_and_validate_queues_rejects_duplicate_ids_across_done() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;
        let queue_path = ralph_dir.join("queue.json");
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;
        let done_path = ralph_dir.join("done.json");
        let mut done_task = task_with("RQ-0001", TaskStatus::Done, vec!["tag".to_string()]);
        done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        save_queue(
            &done_path,
            &QueueFile {
                version: 1,
                tasks: vec![done_task],
            },
        )?;

        let resolved = Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err =
            load_and_validate_queues(&resolved, true).expect_err("expected duplicate id error");
        assert!(err
            .to_string()
            .contains("Duplicate task ID detected across queue and done"));
        Ok(())
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
        let next = next_id_across(&active, None, "RQ", 4)?;
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
        let next = next_id_across(&active, Some(&done), "RQ", 4)?;
        assert_eq!(next, "RQ-0006");
        Ok(())
    }

    // Pruning tests moved to `queue/prune.rs`.
}
