//! Archive operations for queue tasks.

use crate::contracts::{QueueFile, TaskStatus};
use crate::queue::{load_queue, load_queue_or_default, save_queue, validation};
use crate::timeutil;
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ArchiveReport {
    pub moved_ids: Vec<String>,
}

/// Archive terminal tasks (Done/Rejected) in-memory and stamp timestamps.
pub fn archive_terminal_tasks_in_memory(
    active: &mut QueueFile,
    done: &mut QueueFile,
    now_rfc3339: &str,
) -> Result<ArchiveReport> {
    let now = super::validate::parse_rfc3339_utc(now_rfc3339)?;
    let mut moved_ids = Vec::new();
    let mut remaining = Vec::with_capacity(active.tasks.len());

    for mut task in active.tasks.drain(..) {
        if matches!(task.status, TaskStatus::Done | TaskStatus::Rejected) {
            if task.completed_at.is_none() {
                task.completed_at = Some(now.clone());
            }
            task.updated_at = Some(now.clone());
            moved_ids.push(task.id.trim().to_string());
            done.tasks.push(task);
        } else {
            remaining.push(task);
        }
    }

    active.tasks = remaining;

    Ok(ArchiveReport { moved_ids })
}

pub fn archive_done_tasks(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<ArchiveReport> {
    let mut active = load_queue(queue_path)?;
    let mut done = load_queue_or_default(done_path)?;

    validation::validate_queue_set(&active, Some(&done), id_prefix, id_width)?;

    let now = timeutil::now_utc_rfc3339()?;
    let report = archive_terminal_tasks_in_memory(&mut active, &mut done, &now)?;

    if report.moved_ids.is_empty() {
        return Ok(report);
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(report)
}
