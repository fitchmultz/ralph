//! Queue file loading functionality with explicit read-vs-repair semantics.
//!
//! Responsibilities:
//! - Load queue files from disk with standard JSONC parsing.
//! - Load with automatic repair for common JSON errors.
//! - Load with repair and semantic validation without mutating files.
//! - Provide explicit queue-set repair helpers that persist normalized timestamp maintenance.
//!
//! Not handled here:
//! - Queue file saving (see `queue::save`).
//! - ID generation or backup management.
//!
//! Invariants/assumptions:
//! - Missing queue files return default empty queues.
//! - Pure load/validate APIs never write to disk.
//! - Callers must hold locks before invoking explicit repair-and-save APIs.

use crate::config::Resolved;
use crate::contracts::{QueueFile, Task};
use crate::queue::json_repair::attempt_json_repair;
use crate::queue::validation::{self, ValidationWarning};
use anyhow::{Context, Result};
use std::path::Path;
use time::UtcOffset;

#[derive(Debug, Default, Clone, Copy)]
struct QueueMaintenanceReport {
    normalized_timestamps: usize,
    backfilled_completed_at: usize,
    queue_changed: bool,
    done_changed: bool,
}

impl QueueMaintenanceReport {
    fn has_changes(self) -> bool {
        self.normalized_timestamps > 0 || self.backfilled_completed_at > 0
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct SingleQueueMaintenance {
    normalized_timestamps: usize,
    backfilled_completed_at: usize,
    changed: bool,
}

fn normalize_timestamp_field(field: &mut Option<String>) -> Result<bool> {
    let Some(raw) = field.as_ref() else {
        return Ok(false);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }

    let dt = match crate::timeutil::parse_rfc3339(trimmed) {
        Ok(dt) => dt,
        Err(_) => return Ok(false),
    };

    if dt.offset() == UtcOffset::UTC {
        return Ok(false);
    }

    let normalized = crate::timeutil::format_rfc3339(dt)?;
    if normalized == *raw {
        return Ok(false);
    }
    *field = Some(normalized);
    Ok(true)
}

fn normalize_task_timestamps(task: &mut Task) -> Result<usize> {
    let mut normalized = 0usize;

    if normalize_timestamp_field(&mut task.created_at)? {
        normalized += 1;
    }
    if normalize_timestamp_field(&mut task.updated_at)? {
        normalized += 1;
    }
    if normalize_timestamp_field(&mut task.completed_at)? {
        normalized += 1;
    }
    if normalize_timestamp_field(&mut task.started_at)? {
        normalized += 1;
    }
    if normalize_timestamp_field(&mut task.scheduled_start)? {
        normalized += 1;
    }

    Ok(normalized)
}

fn maintain_single_queue_timestamps(
    queue: &mut QueueFile,
    now_utc: &str,
) -> Result<SingleQueueMaintenance> {
    let mut normalized_timestamps = 0usize;
    for task in &mut queue.tasks {
        normalized_timestamps += normalize_task_timestamps(task)?;
    }

    let backfilled_completed_at = super::backfill_terminal_completed_at(queue, now_utc);
    let changed = normalized_timestamps > 0 || backfilled_completed_at > 0;

    Ok(SingleQueueMaintenance {
        normalized_timestamps,
        backfilled_completed_at,
        changed,
    })
}

fn log_maintenance_report(report: QueueMaintenanceReport, queue_path: &Path, done_path: &Path) {
    if !report.has_changes() {
        return;
    }

    log::warn!(
        "Queue repair applied: normalized {} non-UTC timestamp(s), backfilled {} terminal completed_at value(s). Saved queue={}, done={} (queue_path={}, done_path={}).",
        report.normalized_timestamps,
        report.backfilled_completed_at,
        report.queue_changed,
        report.done_changed,
        queue_path.display(),
        done_path.display()
    );
}

fn maintain_and_save_loaded_queues(
    queue_path: &Path,
    queue_file: &mut QueueFile,
    done_path: &Path,
    done_path_exists: bool,
    done_file: &mut QueueFile,
) -> Result<QueueMaintenanceReport> {
    let now = crate::timeutil::now_utc_rfc3339()?;

    let queue_report = maintain_single_queue_timestamps(queue_file, &now)?;
    let done_report = maintain_single_queue_timestamps(done_file, &now)?;

    if queue_report.changed {
        super::save_queue(queue_path, queue_file)
            .with_context(|| format!("save auto-repaired queue {}", queue_path.display()))?;
    }
    if done_report.changed && (done_path_exists || !done_file.tasks.is_empty()) {
        super::save_queue(done_path, done_file)
            .with_context(|| format!("save auto-repaired done {}", done_path.display()))?;
    }

    let report = QueueMaintenanceReport {
        normalized_timestamps: queue_report.normalized_timestamps
            + done_report.normalized_timestamps,
        backfilled_completed_at: queue_report.backfilled_completed_at
            + done_report.backfilled_completed_at,
        queue_changed: queue_report.changed,
        done_changed: done_report.changed,
    };

    log_maintenance_report(report, queue_path, done_path);
    Ok(report)
}

/// Load queue from path, returning default if file doesn't exist.
pub fn load_queue_or_default(path: &Path) -> Result<QueueFile> {
    if !path.exists() {
        return Ok(QueueFile::default());
    }
    load_queue(path)
}

/// Load queue from path with standard JSONC parsing.
pub fn load_queue(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;
    let queue = crate::jsonc::parse_jsonc::<QueueFile>(&raw, &format!("queue {}", path.display()))?;
    Ok(queue)
}

/// Load queue with automatic repair for common JSON errors.
/// Attempts to fix trailing commas and other common agent-induced mistakes.
pub fn load_queue_with_repair(path: &Path) -> Result<QueueFile> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;

    // Try JSONC parsing first (handles both valid JSON and JSONC with comments)
    match crate::jsonc::parse_jsonc::<QueueFile>(&raw, &format!("queue {}", path.display())) {
        Ok(queue) => Ok(queue),
        Err(parse_err) => {
            // Attempt to repair common JSON errors
            log::warn!("Queue JSON parse error, attempting repair: {}", parse_err);

            if let Some(repaired) = attempt_json_repair(&raw) {
                match crate::jsonc::parse_jsonc::<QueueFile>(
                    &repaired,
                    &format!("repaired queue {}", path.display()),
                ) {
                    Ok(queue) => {
                        log::info!("Successfully repaired queue JSON");
                        Ok(queue)
                    }
                    Err(repair_err) => {
                        // Repair failed, return original error with context
                        Err(parse_err).with_context(|| {
                            format!(
                                "parse queue {} as JSON/JSONC (repair also failed: {})",
                                path.display(),
                                repair_err
                            )
                        })?
                    }
                }
            } else {
                // No repair possible, return original error
                Err(parse_err)
            }
        }
    }
}

/// Load queue with JSON repair and semantic validation.
///
/// This API is pure with respect to the filesystem: it may repair parseable JSON
/// mistakes in memory, but it never rewrites the queue file on disk.
///
/// Returns the queue file and any validation warnings (non-blocking issues).
pub fn load_queue_with_repair_and_validate(
    path: &Path,
    done: Option<&crate::contracts::QueueFile>,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
) -> Result<(QueueFile, Vec<ValidationWarning>)> {
    let queue = load_queue_with_repair(path)?;

    let warnings = if let Some(d) = done {
        validation::validate_queue_set(&queue, Some(d), id_prefix, id_width, max_dependency_depth)
            .with_context(|| format!("validate repaired queue {}", path.display()))?
    } else {
        validation::validate_queue(&queue, id_prefix, id_width)
            .with_context(|| format!("validate repaired queue {}", path.display()))?;
        Vec::new()
    };

    Ok((queue, warnings))
}

fn validate_loaded_queues(
    resolved: &Resolved,
    queue_file: &QueueFile,
    done_file: &QueueFile,
) -> Result<Vec<ValidationWarning>> {
    let done_ref = if !done_file.tasks.is_empty() || resolved.done_path.exists() {
        Some(done_file)
    } else {
        None
    };

    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    let warnings = validation::validate_queue_set(
        queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;
    validation::log_warnings(&warnings);
    Ok(warnings)
}

fn load_queue_set_with_repair(
    resolved: &Resolved,
    include_done: bool,
) -> Result<(QueueFile, QueueFile, bool)> {
    let queue_file = load_queue_with_repair(&resolved.queue_path)?;
    let done_path_exists = resolved.done_path.exists();
    let done_file = if done_path_exists {
        load_queue_with_repair(&resolved.done_path)?
    } else {
        QueueFile::default()
    };

    let done_file = if include_done || done_path_exists {
        done_file
    } else {
        QueueFile::default()
    };

    Ok((queue_file, done_file, done_path_exists))
}

/// Load the active queue and optionally the done queue, validating both.
///
/// This API is pure with respect to the filesystem: it may repair parseable JSON
/// in memory, but it never rewrites queue/done files during the read.
pub fn load_and_validate_queues(
    resolved: &Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    let (queue_file, done_for_validation, _done_path_exists) =
        load_queue_set_with_repair(resolved, include_done)?;
    validate_loaded_queues(resolved, &queue_file, &done_for_validation)?;

    let done_file = if include_done {
        Some(done_for_validation)
    } else {
        None
    };

    Ok((queue_file, done_file))
}

/// Explicitly repair queue/done timestamp maintenance and persist the result before validation.
///
/// Unlike [`load_and_validate_queues`], this API mutates queue/done files on disk when it
/// normalizes non-UTC timestamps or backfills missing terminal `completed_at` values.
pub fn repair_and_validate_queues(
    resolved: &Resolved,
    include_done: bool,
) -> Result<(QueueFile, Option<QueueFile>)> {
    let (mut queue_file, mut done_for_validation, done_path_exists) =
        load_queue_set_with_repair(resolved, true)?;

    maintain_and_save_loaded_queues(
        &resolved.queue_path,
        &mut queue_file,
        &resolved.done_path,
        done_path_exists,
        &mut done_for_validation,
    )?;

    validate_loaded_queues(resolved, &queue_file, &done_for_validation)?;

    let done_file = if include_done {
        Some(done_for_validation)
    } else {
        None
    };

    Ok((queue_file, done_file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use crate::fsutil;
    use std::collections::HashMap;
    use tempfile::TempDir;

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

    fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
        let rendered = serde_json::to_string_pretty(queue).context("serialize queue JSON")?;
        fsutil::write_atomic(path, rendered.as_bytes())
            .with_context(|| format!("write queue JSON {}", path.display()))?;
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
        save_queue(
            &done_path,
            &QueueFile {
                version: 1,
                tasks: vec![{
                    let mut t = task("RQ-0001");
                    t.status = TaskStatus::Done;
                    t.completed_at = Some("2026-01-18T00:00:00Z".to_string());
                    t
                }],
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
        assert!(
            err.to_string()
                .contains("Duplicate task ID detected across queue and done")
        );
        Ok(())
    }

    #[test]
    fn load_and_validate_queues_rejects_invalid_deps_when_include_done_false() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        // Queue with invalid dependency (depends on non-existent task)
        let queue_path = ralph_dir.join("queue.json");
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![{
                    let mut t = task("RQ-0001");
                    t.depends_on = vec!["RQ-9999".to_string()]; // Non-existent task!
                    t
                }],
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

        // With include_done=false, should STILL fail on invalid dependency
        // This is the regression test for RQ-0881
        let err = load_and_validate_queues(&resolved, false)
            .expect_err("should fail on invalid dependency");
        assert!(
            err.to_string().contains("Invalid dependency"),
            "Error should mention invalid dependency: {}",
            err
        );

        Ok(())
    }

    #[test]
    fn load_and_validate_queues_rejects_non_utc_timestamps_without_persisting() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let mut active_task = task("RQ-0001");
        active_task.created_at = Some("2026-01-18T12:00:00-05:00".to_string());
        active_task.updated_at = Some("2026-01-18T13:00:00-05:00".to_string());
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![active_task],
            },
        )?;

        let mut done_task = task("RQ-0002");
        done_task.status = TaskStatus::Done;
        done_task.created_at = Some("2026-01-18T10:00:00-07:00".to_string());
        done_task.updated_at = Some("2026-01-18T11:00:00-07:00".to_string());
        done_task.completed_at = Some("2026-01-18T12:00:00-07:00".to_string());
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
            queue_path: queue_path.clone(),
            done_path: done_path.clone(),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = load_and_validate_queues(&resolved, true)
            .expect_err("non-UTC timestamps should fail without explicit repair");
        let err_msg = format!("{err:#}");
        assert!(
            err_msg.contains("must be a valid RFC3339 UTC timestamp"),
            "unexpected error message: {err_msg}"
        );

        let persisted_queue = load_queue(&queue_path)?;
        let persisted_done = load_queue(&done_path)?;
        assert_eq!(
            persisted_queue.tasks[0].created_at.as_deref(),
            Some("2026-01-18T12:00:00-05:00")
        );
        assert_eq!(
            persisted_done.tasks[0].completed_at.as_deref(),
            Some("2026-01-18T12:00:00-07:00")
        );

        Ok(())
    }

    #[test]
    fn load_and_validate_queues_rejects_missing_terminal_completed_at_without_persisting()
    -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let mut queue_task = task("RQ-0001");
        queue_task.status = TaskStatus::Done;
        queue_task.completed_at = None;
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![queue_task],
            },
        )?;
        save_queue(&done_path, &QueueFile::default())?;

        let resolved = Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path: queue_path.clone(),
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = load_and_validate_queues(&resolved, true)
            .expect_err("missing completed_at should fail without explicit repair");
        let err_msg = format!("{err:#}");
        assert!(
            err_msg.contains("Missing completed_at"),
            "unexpected error message: {err_msg}"
        );

        let persisted_queue = load_queue(&queue_path)?;
        assert!(
            persisted_queue.tasks[0].completed_at.is_none(),
            "read-only validation must not backfill completed_at"
        );

        Ok(())
    }

    #[test]
    fn repair_and_validate_queues_normalizes_non_utc_timestamps_and_persists() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let mut active_task = task("RQ-0001");
        active_task.created_at = Some("2026-01-18T12:00:00-05:00".to_string());
        active_task.updated_at = Some("2026-01-18T13:00:00-05:00".to_string());
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![active_task],
            },
        )?;

        let mut done_task = task("RQ-0002");
        done_task.status = TaskStatus::Done;
        done_task.created_at = Some("2026-01-18T10:00:00-07:00".to_string());
        done_task.updated_at = Some("2026-01-18T11:00:00-07:00".to_string());
        done_task.completed_at = Some("2026-01-18T12:00:00-07:00".to_string());
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
            queue_path: queue_path.clone(),
            done_path: done_path.clone(),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let (queue, done) = repair_and_validate_queues(&resolved, true)?;
        let done = done.expect("done file should be present");

        let expected_active_created = crate::timeutil::format_rfc3339(
            crate::timeutil::parse_rfc3339("2026-01-18T12:00:00-05:00")?,
        )?;
        let expected_done_completed = crate::timeutil::format_rfc3339(
            crate::timeutil::parse_rfc3339("2026-01-18T12:00:00-07:00")?,
        )?;

        assert_eq!(
            queue.tasks[0].created_at.as_deref(),
            Some(expected_active_created.as_str())
        );
        assert_eq!(
            done.tasks[0].completed_at.as_deref(),
            Some(expected_done_completed.as_str())
        );

        let persisted_queue = load_queue(&queue_path)?;
        let persisted_done = load_queue(&done_path)?;
        assert_eq!(
            persisted_queue.tasks[0].created_at.as_deref(),
            Some(expected_active_created.as_str())
        );
        assert_eq!(
            persisted_done.tasks[0].completed_at.as_deref(),
            Some(expected_done_completed.as_str())
        );

        Ok(())
    }

    #[test]
    fn repair_and_validate_queues_backfills_terminal_completed_at_and_persists() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let mut queue_task = task("RQ-0001");
        queue_task.status = TaskStatus::Done;
        queue_task.completed_at = None;
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![queue_task],
            },
        )?;
        save_queue(&done_path, &QueueFile::default())?;

        let resolved = Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path: queue_path.clone(),
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let (queue, _done) = repair_and_validate_queues(&resolved, true)?;
        let completed_at = queue.tasks[0]
            .completed_at
            .as_deref()
            .expect("completed_at should be backfilled");
        crate::timeutil::parse_rfc3339(completed_at)?;

        let persisted_queue = load_queue(&queue_path)?;
        let persisted_completed = persisted_queue.tasks[0]
            .completed_at
            .as_deref()
            .expect("completed_at should be saved");
        crate::timeutil::parse_rfc3339(persisted_completed)?;

        Ok(())
    }

    #[test]
    fn load_and_validate_queues_rejects_malformed_timestamps_without_rewrite() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        let queue_path = ralph_dir.join("queue.json");
        let done_path = ralph_dir.join("done.json");

        let mut bad_task = task("RQ-0001");
        bad_task.created_at = Some("not-a-timestamp".to_string());
        save_queue(
            &queue_path,
            &QueueFile {
                version: 1,
                tasks: vec![bad_task],
            },
        )?;

        let resolved = Resolved {
            config: crate::contracts::Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path: queue_path.clone(),
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let err = load_and_validate_queues(&resolved, false)
            .expect_err("expected malformed timestamp to fail validation");
        let err_msg = format!("{:#}", err);
        assert!(
            err_msg.contains("must be a valid RFC3339 UTC timestamp"),
            "unexpected error message: {err_msg}"
        );

        let persisted = std::fs::read_to_string(&queue_path)?;
        assert!(
            persisted.contains("not-a-timestamp"),
            "malformed timestamp should not be rewritten during conservative repair"
        );

        Ok(())
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
    fn load_queue_with_repair_fixes_complex_malformed_json() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with multiple issues
        let malformed = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',],}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should load with repair
        let queue = load_queue_with_repair(&queue_path)?;
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
        assert_eq!(queue.tasks[0].title, "Test task");
        assert_eq!(queue.tasks[0].tags, vec!["bug"]);

        Ok(())
    }

    // Tests for load_queue_with_repair_and_validate (RQ-0502)

    #[test]
    fn load_queue_with_repair_and_validate_rejects_missing_timestamps() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with trailing comma but missing required timestamps
        let malformed = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': [], 'plan': [],}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should fail validation due to missing created_at/updated_at
        let result = load_queue_with_repair_and_validate(&queue_path, None, "RQ", 4, 10);

        let err = result.expect_err("should fail validation due to missing timestamps");
        // Traverse the error chain to find the root cause
        let err_msg = err
            .chain()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            err_msg.contains("created_at") || err_msg.contains("updated_at"),
            "Error should mention missing timestamp: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn load_queue_with_repair_and_validate_accepts_valid_repair() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write malformed JSON with trailing commas but all required fields present
        let malformed = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': ['observed',], 'plan': ['do thing',], 'created_at': '2026-01-18T00:00:00Z', 'updated_at': '2026-01-18T00:00:00Z',}]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should load with repair and pass validation
        let (queue, warnings) =
            load_queue_with_repair_and_validate(&queue_path, None, "RQ", 4, 10)?;

        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
        assert_eq!(queue.tasks[0].title, "Test task");
        assert_eq!(queue.tasks[0].tags, vec!["bug"]);
        assert!(warnings.is_empty());

        Ok(())
    }

    #[test]
    fn load_queue_with_repair_and_validate_detects_done_queue_issues() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let done_path = temp.path().join("done.json");

        // Active queue: valid but with dependency on done task
        let active_malformed = r#"{'version': 1, 'tasks': [{'id': 'RQ-0002', 'title': 'Second task', 'status': 'todo', 'tags': ['bug',], 'scope': ['file',], 'evidence': [], 'plan': [], 'created_at': '2026-01-18T00:00:00Z', 'updated_at': '2026-01-18T00:00:00Z', 'depends_on': ['RQ-0001',],}]}"#;
        std::fs::write(&queue_path, active_malformed)?;

        // Done queue: contains the dependency target
        let done_queue = QueueFile {
            version: 1,
            tasks: vec![{
                let mut t = task("RQ-0001");
                t.status = TaskStatus::Done;
                t.completed_at = Some("2026-01-18T00:00:00Z".to_string());
                t
            }],
        };
        save_queue(&done_path, &done_queue)?;

        // Should load with repair and validate successfully
        let (queue, warnings) =
            load_queue_with_repair_and_validate(&queue_path, Some(&done_queue), "RQ", 4, 10)?;

        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0002");
        assert!(warnings.is_empty());

        Ok(())
    }

    #[test]
    fn load_queue_accepts_scalar_custom_fields_and_save_normalizes_to_strings() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write queue with numeric and boolean custom_fields values
        std::fs::write(
            &queue_path,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","title":"t","created_at":"2026-01-18T00:00:00Z","updated_at":"2026-01-18T00:00:00Z","custom_fields":{"n":1411,"b":false}}]}"#,
        )?;

        // Load queue - should accept numeric/boolean values and coerce to strings
        let queue = load_queue(&queue_path)?;
        assert_eq!(
            queue.tasks[0].custom_fields.get("n").map(String::as_str),
            Some("1411")
        );
        assert_eq!(
            queue.tasks[0].custom_fields.get("b").map(String::as_str),
            Some("false")
        );

        // Save queue - should serialize as strings
        save_queue(&queue_path, &queue)?;
        let rendered = std::fs::read_to_string(&queue_path)?;
        assert!(rendered.contains("\"n\": \"1411\""));
        assert!(rendered.contains("\"b\": \"false\""));

        Ok(())
    }

    #[test]
    fn load_queue_malformed_json_returns_error() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write unrecoverably malformed JSON (not fixable by repair)
        let malformed = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": }]}"#;
        std::fs::write(&queue_path, malformed)?;

        // Should fail with descriptive error
        let result = load_queue(&queue_path);
        assert!(result.is_err(), "Should error on malformed JSON");
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("parse") || err_msg.contains("JSON"),
            "Error should mention parsing/JSON: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn load_queue_with_repair_fails_on_unrepairable_json() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write JSON that is too corrupted to repair (structurally invalid)
        let unrepairable = r#"{this is not valid json at all"#;
        std::fs::write(&queue_path, unrepairable)?;

        // Should fail even with repair attempt
        let result = load_queue_with_repair(&queue_path);
        assert!(result.is_err(), "Should error on unrepairable JSON");
        let err = result.unwrap_err();
        let err_msg = format!("{:#}", err);
        assert!(
            err_msg.contains("parse") || err_msg.contains("JSON") || err_msg.contains("repair"),
            "Error should mention parsing or repair failure: {}",
            err_msg
        );

        Ok(())
    }

    #[test]
    fn load_queue_handles_empty_file() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Write empty file
        std::fs::write(&queue_path, "")?;

        // Should fail gracefully with meaningful error
        let result = load_queue(&queue_path);
        assert!(result.is_err(), "Should error on empty file");
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("EOF") || err_msg.contains("parse") || err_msg.contains("empty"),
            "Error should indicate empty or unparseable file: {}",
            err_msg
        );

        Ok(())
    }

    /// Test: Truncated JSON file (simulating partial write or crash during write)
    /// Scenario: File ends mid-object due to external corruption or power loss
    /// Expected: load_queue should detect and report a parsing/EOF error
    #[test]
    fn load_queue_detects_truncated_file() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Simulate truncated write - valid JSON cut off mid-stream
        let truncated = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test""#;
        std::fs::write(&queue_path, truncated)?;

        let result = load_queue(&queue_path);
        assert!(result.is_err(), "Should error on truncated JSON");
        let err_msg = format!("{:#}", result.unwrap_err());
        assert!(
            err_msg.contains("EOF")
                || err_msg.contains("unexpected end")
                || err_msg.contains("parse"),
            "Error should indicate truncated file or EOF: {}",
            err_msg
        );

        Ok(())
    }
}
