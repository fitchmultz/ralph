//! Task queue persistence, validation, and pruning.
//!
//! This module handles loading, saving, and validating task queues stored
//! as JSON files (.ralph/queue.json for active tasks, .ralph/done.json
//! for completed tasks). It provides operations for moving completed tasks,
//! updating task status, repairing queue data, and pruning old tasks from
//! the done archive.

use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::fsutil;
use crate::timeutil;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub mod operations;
pub mod prune;
pub mod search;
pub mod validation;

pub use operations::*;
pub use prune::{prune_done_tasks, PruneOptions, PruneReport};
pub use search::{filter_tasks, search_tasks};
pub use validation::{are_dependencies_met, validate_queue, validate_queue_set};

// Pruning types live in `queue::prune` (re-exported from this module).

#[derive(Debug, Default, Clone)]
pub struct RepairReport {
    pub fixed_tasks: usize,
    pub remapped_ids: Vec<(String, String)>,
    pub fixed_timestamps: usize,
}

impl RepairReport {
    pub fn is_empty(&self) -> bool {
        self.fixed_tasks == 0 && self.remapped_ids.is_empty() && self.fixed_timestamps == 0
    }
}

pub fn repair_queue(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
    dry_run: bool,
) -> Result<RepairReport> {
    let mut active = load_queue_or_default(queue_path)?;
    let mut done = load_queue_or_default(done_path)?;

    let mut report = RepairReport::default();
    let expected_prefix = normalize_prefix(id_prefix);
    let now = timeutil::now_utc_rfc3339_or_fallback();

    // 1. Scan for max ID to ensure new IDs don't collide
    let mut max_id_val: u32 = 0;
    let mut scan_max = |tasks: &[Task]| {
        for task in tasks {
            if let Ok(val) = validation::validate_task_id(0, &task.id, &expected_prefix, id_width) {
                max_id_val = max_id_val.max(val);
            }
        }
    };
    scan_max(&active.tasks);
    scan_max(&done.tasks);

    let mut next_id_val = max_id_val + 1;
    let mut seen_ids = HashSet::new();

    // Helper to repair a list of tasks
    let mut repair_tasks = |tasks: &mut Vec<Task>| {
        for task in tasks.iter_mut() {
            let mut modified = false;

            // Fix missing fields
            if task.title.trim().is_empty() {
                task.title = "Untitled".to_string();
                modified = true;
            }
            if task.tags.is_empty() {
                task.tags.push("untagged".to_string());
                modified = true;
            }
            if task.scope.is_empty() {
                task.scope.push("unknown".to_string());
                modified = true;
            }
            if task.evidence.is_empty() {
                task.evidence.push("None provided".to_string());
                modified = true;
            }
            if task.plan.is_empty() {
                task.plan.push("To be determined".to_string());
                modified = true;
            }
            if task.request.as_ref().is_none_or(|r| r.trim().is_empty()) {
                task.request = Some("Imported task".to_string());
                modified = true;
            }

            // Fix timestamps
            let mut fix_ts = |ts: &mut Option<String>, label: &str| {
                if let Some(val) = ts {
                    if OffsetDateTime::parse(val, &Rfc3339).is_err() {
                        *ts = Some(now.clone());
                        report.fixed_timestamps += 1;
                    }
                } else {
                    // Create/Update required
                    if label == "created_at" || label == "updated_at" {
                        *ts = Some(now.clone());
                        report.fixed_timestamps += 1;
                    }
                }
            };
            fix_ts(&mut task.created_at, "created_at");
            fix_ts(&mut task.updated_at, "updated_at");
            if task.status == TaskStatus::Done || task.status == TaskStatus::Rejected {
                fix_ts(&mut task.completed_at, "completed_at");
            }

            if modified {
                report.fixed_tasks += 1;
            }

            // Fix ID
            // We use a normalized key for collision detection
            let id_key = task.id.trim().to_uppercase();
            let is_valid_format =
                validation::validate_task_id(0, &task.id, &expected_prefix, id_width).is_ok();

            if !is_valid_format || seen_ids.contains(&id_key) || id_key.is_empty() {
                let new_id = format_id(&expected_prefix, next_id_val, id_width);
                next_id_val += 1;
                report.remapped_ids.push((task.id.clone(), new_id.clone()));
                task.id = new_id.clone();
                seen_ids.insert(new_id);
            } else {
                seen_ids.insert(id_key);
            }
        }
    };

    repair_tasks(&mut active.tasks);
    repair_tasks(&mut done.tasks);

    if !dry_run && !report.is_empty() {
        save_queue(queue_path, &active)?;
        save_queue(done_path, &done)?;
    }
    Ok(report)
}

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

fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().to_uppercase()
}

fn format_id(prefix: &str, number: u32, width: usize) -> String {
    format!("{}-{:0width$}", prefix, number, width = width)
}

// Pruning implementation moved to `queue::prune` (re-exported from this module).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;

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
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0001", TaskStatus::Todo, vec!["tag".to_string()]),
                task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]),
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
        let done = QueueFile {
            version: 1,
            tasks: vec![
                task_with("RQ-0005", TaskStatus::Done, vec!["tag".to_string()]),
                task_with("RQ-0009", TaskStatus::Rejected, vec!["tag".to_string()]),
            ],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4)?;
        assert_eq!(next, "RQ-0006");
        Ok(())
    }

    // Pruning tests moved to `queue/prune.rs`.
}
