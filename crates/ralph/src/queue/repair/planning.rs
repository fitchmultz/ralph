//! Purpose: Plan queue repairs from on-disk and in-memory queue state.
//!
//! Responsibilities:
//! - Load the active and done queues for repair planning.
//! - Decide which tasks need missing-field, timestamp, or ID repairs based on
//!   the requested `RepairScope`.
//! - Track remapped IDs and rewrite cross-task references in a second pass.
//! - Surface the resulting `QueueRepairPlan` and `RepairReport` for callers.
//!
//! Scope:
//! - Pure planning only; never validates the planned queue set or writes to disk.
//! - Apply, validation, and persistence flows live in `apply.rs`.
//!
//! Usage:
//! - `plan_queue_repair` and `plan_queue_maintenance_repair` load from disk and
//!   delegate to `plan_loaded_queue_repair_with_scope`.
//! - `plan_loaded_queue_repair` is the in-memory entry point used by callers
//!   that already have queue state loaded.
//!
//! Invariants/Assumptions:
//! - Maintenance scope only normalizes timestamps; full scope additionally fills
//!   missing fields and remaps invalid/duplicate IDs.
//! - Non-UTC RFC3339 timestamps are normalized in both scopes.
//! - The planner increments `report.fixed_tasks` once per task that was modified
//!   in the first pass, and again per task whose relationships were rewritten in
//!   the second pass; this preserves historical operation-count semantics.

use super::relationships::rewrite_task_id_references;
use super::types::{QueueRepairPlan, RepairReport, RepairScope};
use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::queue::{format_id, load_queue_or_default, normalize_prefix, validation};
use crate::timeutil;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use time::UtcOffset;

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

pub(super) fn plan_loaded_queue_repair_with_scope(
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

fn parse_id_number(id: &str, expected_prefix: &str) -> Option<u32> {
    let normalized = id.trim().to_uppercase();
    let prefix = format!("{}-", expected_prefix);
    let suffix = normalized.strip_prefix(&prefix)?;
    suffix.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use crate::queue::save_queue;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn task(id: &str) -> Task {
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
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            parent_id: None,
        }
    }

    #[test]
    fn plan_repair_backfills_completed_at_for_done_tasks() {
        let dir = tempdir().unwrap();
        let queue_path = dir.path().join("queue.json");
        let done_path = dir.path().join("done.json");

        let mut t = task("RQ-0001");
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
        let dir = tempdir().unwrap();
        let queue_path = dir.path().join("queue.json");
        let done_path = dir.path().join("done.json");

        let mut t = task("RQ-0001");
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
