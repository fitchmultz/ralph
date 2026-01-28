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

use crate::config::Resolved;
use crate::contracts::{QueueFile, TaskStatus};
use crate::{fsutil, lock};
use anyhow::{Context, Result};
use std::path::Path;

pub mod graph;
pub mod operations;
pub mod prune;
pub mod repair;
pub mod search;
pub mod validation;

pub use graph::*;
pub use operations::*;
pub use prune::{prune_done_tasks, PruneOptions, PruneReport};
pub use repair::*;
pub use search::{filter_tasks, search_tasks, SearchOptions};
pub use validation::{validate_queue, validate_queue_set};

// Pruning types live in `queue::prune` (re-exported from this module).

pub fn acquire_queue_lock(repo_root: &Path, label: &str, force: bool) -> Result<lock::DirLock> {
    let lock_dir = lock::queue_lock_dir(repo_root);
    lock::acquire_dir_lock(&lock_dir, label, force)
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

/// Load queue with automatic repair for common JSON errors.
/// Attempts to fix trailing commas and other common agent-induced mistakes.
pub fn load_queue_with_repair(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;

    match serde_json::from_str::<QueueFile>(&raw) {
        Ok(queue) => Ok(queue),
        Err(parse_err) => {
            // Attempt to repair common JSON errors
            log::warn!("Queue JSON parse error, attempting repair: {}", parse_err);

            if let Some(repaired) = attempt_json_repair(&raw) {
                match serde_json::from_str::<QueueFile>(&repaired) {
                    Ok(queue) => {
                        log::info!("Successfully repaired queue JSON");
                        Ok(queue)
                    }
                    Err(repair_err) => {
                        // Repair failed, return original error with context
                        Err(parse_err).with_context(|| {
                            format!(
                                "parse queue {} as JSON (repair also failed: {})",
                                path.display(),
                                repair_err
                            )
                        })?
                    }
                }
            } else {
                // No repair possible, return original error
                Err(parse_err).with_context(|| format!("parse queue {} as JSON", path.display()))?
            }
        }
    }
}

/// Attempt to repair common JSON errors induced by agents.
/// Returns Some(repaired_json) if repairs were made, None if no repairs possible.
pub fn attempt_json_repair(raw: &str) -> Option<String> {
    let mut repaired = raw.to_string();
    let original = raw.to_string();

    // Repair 1: Remove trailing commas before ] or }
    // Pattern: ,\s*] or ,\s*}
    repaired = regex::Regex::new(r",(\s*[}\]])")
        .ok()?
        .replace_all(&repaired, "$1")
        .to_string();

    // Repair 2: Remove trailing commas at end of arrays/objects (more aggressive)
    // This handles cases where there might be newlines between comma and bracket
    // Pattern: ,(\s*)\n(\s*[}\]])
    repaired = regex::Regex::new(r",(\s*)\n(\s*[}\]])")
        .ok()?
        .replace_all(&repaired, "$1\n$2")
        .to_string();

    // Repair 3: Fix missing closing bracket at end of file
    let open_brackets = repaired.matches('[').count();
    let close_brackets = repaired.matches(']').count();
    let open_braces = repaired.matches('{').count();
    let close_braces = repaired.matches('}').count();

    if open_brackets > close_brackets {
        repaired.push_str(&"]".repeat(open_brackets - close_brackets));
    }
    if open_braces > close_braces {
        repaired.push_str(&"}".repeat(open_braces - close_braces));
    }

    if repaired != original {
        Some(repaired)
    } else {
        None
    }
}

/// Create a backup of the queue file before modification.
/// Returns the path to the backup file.
pub fn backup_queue(path: &Path, backup_dir: &Path) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(backup_dir)?;
    let timestamp = crate::timeutil::now_utc_rfc3339_or_fallback().replace([':', '.'], "-");
    let backup_name = format!("queue.json.backup.{}", timestamp);
    let backup_path = backup_dir.join(backup_name);

    std::fs::copy(path, &backup_path)
        .with_context(|| format!("backup queue to {}", backup_path.display()))?;

    Ok(backup_path)
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
        let mut done_task = task_with("RQ-0009", TaskStatus::Done, vec!["tag".to_string()]);
        done_task.completed_at = Some("2026-01-18T00:00:00Z".to_string());
        let done = QueueFile {
            version: 1,
            tasks: vec![done_task],
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

    #[test]
    fn attempt_json_repair_fixes_multiple_trailing_commas() {
        // Test with a complete valid task structure that includes all required fields
        let input = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test", "status": "todo", "tags": ["a", "b",], "scope": ["file",],}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
    }

    #[test]
    fn load_queue_with_repair_fixes_malformed_json() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with trailing comma
        let malformed = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test", "status": "todo", "tags": ["bug",],}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should load with repair
        let queue = load_queue_with_repair(&queue_path)?;
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
        assert_eq!(queue.tasks[0].tags, vec!["bug"]);

        Ok(())
    }

    #[test]
    fn backup_queue_creates_backup_file() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let backup_dir = temp.path().join("backups");

        // Create initial queue
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![task("RQ-0001")],
            },
        )?;

        // Create backup
        let backup_path = backup_queue(&queue_path, &backup_dir)?;

        // Verify backup exists and contains valid JSON
        assert!(backup_path.exists());
        let backup_queue: QueueFile =
            serde_json::from_str(&std::fs::read_to_string(&backup_path)?)?;
        assert_eq!(backup_queue.tasks.len(), 1);
        assert_eq!(backup_queue.tasks[0].id, "RQ-0001");

        Ok(())
    }
}
