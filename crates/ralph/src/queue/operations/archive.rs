//! Archive operations for queue tasks.

use crate::contracts::TaskStatus;
use crate::queue::{load_queue, load_queue_or_default, save_queue, validation};
use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ArchiveReport {
    pub moved_ids: Vec<String>,
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

    let mut moved_ids = Vec::new();
    let mut remaining = Vec::new();

    for task in active.tasks.into_iter() {
        if task.status != TaskStatus::Done && task.status != TaskStatus::Rejected {
            remaining.push(task);
            continue;
        }

        let key = task.id.trim().to_string();
        moved_ids.push(key);
        done.tasks.push(task);
    }

    active.tasks = remaining;

    if moved_ids.is_empty() {
        return Ok(ArchiveReport { moved_ids });
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(ArchiveReport { moved_ids })
}
