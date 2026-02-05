//! Queue repair and dependency traversal.
//!
//! This module consolidates logic for repairing queue inconsistencies (missing fields,
//! duplicate IDs, invalid/missing timestamps) and for traversing task dependency graphs
//! (e.g., computing all tasks that depend on a given task ID).

use super::{format_id, load_queue_or_default, normalize_prefix, save_queue, validation};
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::timeutil;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use time::UtcOffset;

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
                    match timeutil::parse_rfc3339(val) {
                        Ok(dt) => {
                            if dt.offset() != UtcOffset::UTC {
                                let normalized =
                                    timeutil::format_rfc3339(dt).unwrap_or_else(|_| now.clone());
                                *ts = Some(normalized);
                                report.fixed_timestamps += 1;
                            }
                        }
                        Err(_) => {
                            *ts = Some(now.clone());
                            report.fixed_timestamps += 1;
                        }
                    }
                } else {
                    // Create/Update required
                    if label == "created_at" || label == "updated_at" || label == "completed_at" {
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

    // Second pass: Update dependencies for remapped IDs
    if !report.remapped_ids.is_empty() {
        let remapped_map: std::collections::HashMap<String, String> =
            report.remapped_ids.iter().cloned().collect();

        let mut fix_dependencies = |tasks: &mut Vec<Task>| {
            for task in tasks.iter_mut() {
                let mut deps_modified = false;
                for dep in task.depends_on.iter_mut() {
                    if let Some(new_id) = remapped_map.get(dep) {
                        *dep = new_id.clone();
                        deps_modified = true;
                    }
                }
                if deps_modified {
                    // Only count as fixed if we haven't already counted it (simpler: just increment,
                    // as 'fixed_tasks' is a count of tasks touched, strictly speaking we might want to track set of unique modified tasks
                    // but usually a simple counter is enough for the report.
                    // However, if we want to be precise: "Fixed missing fields in X tasks" vs "Fixed dependencies".
                    // The report struct just has `fixed_tasks`.
                    // If we modified fields in pass 1 AND deps in pass 2, it's the same task being fixed.
                    // But `repair_tasks` already incremented if fields/ID changed.
                    // To avoid double counting, we could assume `fixed_tasks` is just "operations performed" or track unique indices.
                    // Given the current implementation just increments, let's just increment.
                    report.fixed_tasks += 1;
                }
            }
        };

        fix_dependencies(&mut active.tasks);
        fix_dependencies(&mut done.tasks);
    }

    if !dry_run && !report.is_empty() {
        save_queue(queue_path, &active)?;
        save_queue(done_path, &done)?;
    }
    Ok(report)
}

/// Get all tasks that depend on the given task ID (recursively).
/// Returns a list of task IDs that depend on the root task.
pub fn get_dependents(root_id: &str, active: &QueueFile, done: Option<&QueueFile>) -> Vec<String> {
    let mut dependents = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let root_id = root_id.trim();

    fn collect_dependents(
        task_id: &str,
        active: &QueueFile,
        done: Option<&QueueFile>,
        dependents: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(task_id) {
            return;
        }
        visited.insert(task_id.to_string());

        // Check all tasks in active queue
        for task in &active.tasks {
            let current_id = task.id.trim();
            if task.depends_on.iter().any(|d| d.trim() == task_id) {
                if !dependents.contains(&current_id.to_string()) {
                    dependents.push(current_id.to_string());
                }
                collect_dependents(current_id, active, done, dependents, visited);
            }
        }

        // Check all tasks in done archive
        if let Some(done_file) = done {
            for task in &done_file.tasks {
                let current_id = task.id.trim();
                if task.depends_on.iter().any(|d| d.trim() == task_id) {
                    if !dependents.contains(&current_id.to_string()) {
                        dependents.push(current_id.to_string());
                    }
                    collect_dependents(current_id, active, done, dependents, visited);
                }
            }
        }
    }

    collect_dependents(root_id, active, done, &mut dependents, &mut visited);
    dependents.retain(|id| id != root_id);
    dependents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;

    fn task(id: &str, depends_on: Vec<&str>) -> Task {
        Task {
            id: id.to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags: vec!["test".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["evidence".to_string()],
            plan: vec!["plan".to_string()],
            notes: vec![],
            request: Some("request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: depends_on.into_iter().map(|s| s.to_string()).collect(),
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn get_dependents_traverses_active_and_done_recursively() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task("RQ-0001", vec![]),
                task("RQ-0002", vec!["RQ-0001"]),
                task("RQ-0003", vec!["RQ-0002"]),
            ],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0004", vec!["RQ-0003"])],
        };

        let got = get_dependents("RQ-0001", &active, Some(&done));
        let set: std::collections::HashSet<String> = got.into_iter().collect();

        assert!(set.contains("RQ-0002"));
        assert!(set.contains("RQ-0003"));
        assert!(set.contains("RQ-0004"));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn get_dependents_handles_cycles_without_infinite_recursion() {
        let active = QueueFile {
            version: 1,
            tasks: vec![
                task("RQ-0001", vec!["RQ-0002"]),
                task("RQ-0002", vec!["RQ-0001"]),
            ],
        };

        let got = get_dependents("RQ-0001", &active, None);
        let set: std::collections::HashSet<String> = got.into_iter().collect();

        assert!(set.contains("RQ-0002"));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn repair_backfills_completed_at_for_done_tasks() {
        use crate::queue::save_queue;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let queue_path = dir.path().join("queue.json");
        let done_path = dir.path().join("done.json");

        let mut t = task("RQ-0001", vec![]);
        t.status = TaskStatus::Done;
        t.completed_at = None;

        let active = QueueFile {
            version: 1,
            tasks: vec![t],
        };
        save_queue(&queue_path, &active).unwrap();
        save_queue(
            &done_path,
            &QueueFile {
                version: 1,
                tasks: vec![],
            },
        )
        .unwrap();

        let report = repair_queue(&queue_path, &done_path, "RQ", 4, false).unwrap();
        assert!(report.fixed_timestamps > 0);

        let repaired = crate::queue::load_queue_or_default(&queue_path).unwrap();
        assert!(repaired.tasks[0].completed_at.is_some());
    }

    #[test]
    fn repair_normalizes_non_utc_timestamps() {
        use crate::queue::save_queue;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let queue_path = dir.path().join("queue.json");
        let done_path = dir.path().join("done.json");

        let mut t = task("RQ-0001", vec![]);
        t.status = TaskStatus::Done;
        t.created_at = Some("2026-01-18T12:00:00-05:00".to_string());
        t.updated_at = Some("2026-01-18T12:00:00-05:00".to_string());
        t.completed_at = Some("2026-01-18T12:00:00-05:00".to_string());

        let active = QueueFile {
            version: 1,
            tasks: vec![t],
        };
        save_queue(&queue_path, &active).unwrap();
        save_queue(
            &done_path,
            &QueueFile {
                version: 1,
                tasks: vec![],
            },
        )
        .unwrap();

        let report = repair_queue(&queue_path, &done_path, "RQ", 4, false).unwrap();
        assert!(report.fixed_timestamps > 0);

        let repaired = crate::queue::load_queue_or_default(&queue_path).unwrap();
        let expected = crate::timeutil::format_rfc3339(
            crate::timeutil::parse_rfc3339("2026-01-18T12:00:00-05:00").unwrap(),
        )
        .unwrap();
        assert_eq!(
            repaired.tasks[0].created_at.as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            repaired.tasks[0].updated_at.as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            repaired.tasks[0].completed_at.as_deref(),
            Some(expected.as_str())
        );
    }
}
