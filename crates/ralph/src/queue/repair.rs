//! Purpose: Repair queue files and traverse dependency relationships.
//!
//! Responsibilities:
//! - Normalize recoverable queue inconsistencies such as missing fields, duplicate
//!   IDs, invalid IDs, and invalid or missing timestamps.
//! - Keep remapped task IDs consistent across every relationship field.
//! - Traverse dependency graphs for dependent-task lookup.
//!
//! Scope:
//! - Queue repair and dependency traversal only.
//! - Queue loading/saving and validation policy live in sibling modules.
//!
//! Usage:
//! - CLI, machine, and doctor recovery surfaces plan and apply repair here.
//! - Runtime helpers call `get_dependents` for dependency traversal.
//!
//! Invariants/Assumptions:
//! - Mutating repair requires a held queue lock and creates an undo snapshot before saving.
//! - Mutating repair validates the repaired active/done queue set before saving.

use super::{format_id, load_queue_or_default, normalize_prefix, save_queue, validation};
use crate::config::Resolved;
use crate::constants::queue::DEFAULT_MAX_DEPENDENCY_DEPTH;
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::lock::DirLock;
use crate::timeutil;
use anyhow::Result;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use time::UtcOffset;

#[derive(Debug, Default, Clone, Serialize)]
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

#[derive(Debug, Clone)]
pub struct QueueRepairPlan {
    active: QueueFile,
    done: QueueFile,
    report: RepairReport,
    queue_changed: bool,
    done_changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepairScope {
    Maintenance,
    Full,
}

impl QueueRepairPlan {
    pub fn has_changes(&self) -> bool {
        self.queue_changed || self.done_changed
    }

    pub fn report(&self) -> &RepairReport {
        &self.report
    }

    pub fn into_parts(self) -> (QueueFile, QueueFile, RepairReport) {
        (self.active, self.done, self.report)
    }
}

pub fn apply_queue_repair_with_undo(
    resolved: &Resolved,
    _queue_lock: &DirLock,
    operation: &str,
) -> Result<RepairReport> {
    apply_repair_plan_with_undo(
        resolved,
        operation,
        plan_queue_repair(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?,
    )
}

pub fn apply_queue_maintenance_repair_with_undo(
    resolved: &Resolved,
    _queue_lock: &DirLock,
    operation: &str,
) -> Result<RepairReport> {
    apply_repair_plan_with_undo(
        resolved,
        operation,
        plan_queue_maintenance_repair(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?,
    )
}

fn apply_repair_plan_with_undo(
    resolved: &Resolved,
    operation: &str,
    plan: QueueRepairPlan,
) -> Result<RepairReport> {
    let report = plan.report.clone();

    if !plan.has_changes() {
        return Ok(report);
    }

    validate_repair_plan(&plan, &resolved.id_prefix, resolved.id_width)?;
    crate::undo::create_undo_snapshot(resolved, operation)?;
    save_repair_plan(&resolved.queue_path, &resolved.done_path, &plan)?;
    Ok(report)
}

pub fn plan_queue_repair(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<QueueRepairPlan> {
    let active = load_queue_or_default(queue_path)?;
    let done = load_queue_or_default(done_path)?;
    plan_loaded_queue_repair_with_scope(active, done, id_prefix, id_width, RepairScope::Full)
}

pub fn plan_queue_maintenance_repair(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<QueueRepairPlan> {
    let active = load_queue_or_default(queue_path)?;
    let done = load_queue_or_default(done_path)?;
    plan_loaded_queue_repair_with_scope(active, done, id_prefix, id_width, RepairScope::Maintenance)
}

pub fn plan_loaded_queue_repair(
    active: QueueFile,
    done: QueueFile,
    id_prefix: &str,
    id_width: usize,
) -> Result<QueueRepairPlan> {
    plan_loaded_queue_repair_with_scope(active, done, id_prefix, id_width, RepairScope::Full)
}

fn plan_loaded_queue_repair_with_scope(
    mut active: QueueFile,
    mut done: QueueFile,
    id_prefix: &str,
    id_width: usize,
    scope: RepairScope,
) -> Result<QueueRepairPlan> {
    let mut report = RepairReport::default();
    let expected_prefix = normalize_prefix(id_prefix);
    let now = timeutil::now_utc_rfc3339_or_fallback();

    // Determine max existing numeric ID across active and done.
    let mut max_id_val = 0;
    for task in active.tasks.iter().chain(done.tasks.iter()) {
        if let Some(n) = parse_id_number(&task.id, &expected_prefix) {
            max_id_val = max_id_val.max(n);
        }
    }
    let mut next_id_val = max_id_val + 1;
    let mut seen_ids = HashSet::new();

    let mut repair_tasks = |tasks: &mut Vec<Task>| -> bool {
        let mut queue_changed = false;
        for task in tasks.iter_mut() {
            let mut modified = false;
            let mut timestamp_modified = false;
            let mut id_modified = false;

            if scope == RepairScope::Full {
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
            }

            // Fix timestamps
            let terminal = matches!(task.status, TaskStatus::Done | TaskStatus::Rejected);
            let mut fix_ts = |ts: &mut Option<String>, label: &str| {
                if let Some(existing) = ts.as_ref() {
                    match timeutil::parse_rfc3339(existing) {
                        Ok(dt) => {
                            if dt.offset() != UtcOffset::UTC {
                                let normalized =
                                    timeutil::format_rfc3339(dt).unwrap_or_else(|_| now.clone());
                                *ts = Some(normalized);
                                report.fixed_timestamps += 1;
                                timestamp_modified = true;
                            }
                        }
                        Err(_) => {
                            if scope == RepairScope::Full {
                                *ts = Some(now.clone());
                                report.fixed_timestamps += 1;
                                timestamp_modified = true;
                            }
                        }
                    }
                } else {
                    // Create/Update required
                    let should_backfill = (scope == RepairScope::Full
                        && (label == "created_at"
                            || label == "updated_at"
                            || label == "completed_at"))
                        || (scope == RepairScope::Maintenance && label == "completed_at");
                    if should_backfill {
                        *ts = Some(now.clone());
                        report.fixed_timestamps += 1;
                        timestamp_modified = true;
                    }
                }
            };

            fix_ts(&mut task.created_at, "created_at");
            fix_ts(&mut task.updated_at, "updated_at");
            if terminal || task.completed_at.is_some() {
                fix_ts(&mut task.completed_at, "completed_at");
            }

            if modified || timestamp_modified {
                report.fixed_tasks += 1;
            }

            if scope == RepairScope::Full {
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
                    id_modified = true;
                } else {
                    seen_ids.insert(id_key);
                }
            }

            queue_changed |= modified || timestamp_modified || id_modified;
        }
        queue_changed
    };

    let mut queue_changed = repair_tasks(&mut active.tasks);
    let mut done_changed = repair_tasks(&mut done.tasks);

    // Second pass: update relationship references for remapped IDs.
    if scope == RepairScope::Full && !report.remapped_ids.is_empty() {
        let remapped_map: HashMap<String, String> = report.remapped_ids.iter().cloned().collect();

        let mut fix_relationships = |tasks: &mut Vec<Task>| {
            let mut queue_changed = false;
            for task in tasks.iter_mut() {
                if rewrite_task_id_references(task, &remapped_map) {
                    // Preserve the historical operation count semantics for `fixed_tasks`.
                    report.fixed_tasks += 1;
                    queue_changed = true;
                }
            }
            queue_changed
        };

        queue_changed |= fix_relationships(&mut active.tasks);
        done_changed |= fix_relationships(&mut done.tasks);
    }

    Ok(QueueRepairPlan {
        active,
        done,
        report,
        queue_changed,
        done_changed,
    })
}

fn validate_repair_plan(plan: &QueueRepairPlan, id_prefix: &str, id_width: usize) -> Result<()> {
    validation::validate_queue_set(
        &plan.active,
        Some(&plan.done),
        id_prefix,
        id_width,
        DEFAULT_MAX_DEPENDENCY_DEPTH,
    )?;
    Ok(())
}

fn save_repair_plan(queue_path: &Path, done_path: &Path, plan: &QueueRepairPlan) -> Result<()> {
    if plan.queue_changed {
        save_queue(queue_path, &plan.active)?;
    }
    if plan.done_changed {
        save_queue(done_path, &plan.done)?;
    }
    Ok(())
}

fn parse_id_number(id: &str, expected_prefix: &str) -> Option<u32> {
    let normalized = id.trim().to_uppercase();
    let prefix = format!("{}-", expected_prefix);
    let suffix = normalized.strip_prefix(&prefix)?;
    suffix.parse().ok()
}

fn rewrite_task_id_references(task: &mut Task, remapped_ids: &HashMap<String, String>) -> bool {
    let mut modified = false;
    modified |= rewrite_id_list(&mut task.depends_on, remapped_ids);
    modified |= rewrite_id_list(&mut task.blocks, remapped_ids);
    modified |= rewrite_id_list(&mut task.relates_to, remapped_ids);
    modified |= rewrite_optional_id(&mut task.duplicates, remapped_ids);
    modified |= rewrite_optional_id(&mut task.parent_id, remapped_ids);
    modified
}

fn rewrite_id_list(ids: &mut [String], remapped_ids: &HashMap<String, String>) -> bool {
    let mut modified = false;
    for id in ids {
        if let Some(new_id) = remapped_task_id(id, remapped_ids) {
            *id = new_id;
            modified = true;
        }
    }
    modified
}

fn rewrite_optional_id(id: &mut Option<String>, remapped_ids: &HashMap<String, String>) -> bool {
    let Some(current_id) = id.as_deref() else {
        return false;
    };
    let Some(new_id) = remapped_task_id(current_id, remapped_ids) else {
        return false;
    };

    *id = Some(new_id);
    true
}

fn remapped_task_id(id: &str, remapped_ids: &HashMap<String, String>) -> Option<String> {
    remapped_ids
        .get(id)
        .or_else(|| remapped_ids.get(id.trim()))
        .cloned()
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
            description: None,
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
            estimated_minutes: None,
            actual_minutes: None,
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
    fn plan_repair_backfills_completed_at_for_done_tasks() {
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

        let plan = plan_queue_repair(&queue_path, &done_path, "RQ", 4).unwrap();
        let report = plan.report();
        assert!(report.fixed_timestamps > 0);

        let (repaired, _done, _report) = plan.into_parts();
        assert!(repaired.tasks[0].completed_at.is_some());
    }

    #[test]
    fn plan_repair_normalizes_non_utc_timestamps() {
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

        let plan = plan_queue_repair(&queue_path, &done_path, "RQ", 4).unwrap();
        let report = plan.report();
        assert!(report.fixed_timestamps > 0);

        let (repaired, _done, _report) = plan.into_parts();
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
