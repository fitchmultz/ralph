//! Queue import merge helpers.
//!
//! Purpose:
//! - Queue import merge helpers.
//!
//! Responsibilities:
//! - Apply duplicate handling policy for imported tasks.
//! - Reserve fresh task IDs for missing or renamed tasks.
//! - Append imported tasks and restore queue insertion ordering.
//!
//! Not handled here:
//! - Parsing raw input payloads.
//! - Queue validation or undo snapshot creation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Existing queue and done files are already loaded and validated.
//! - New IDs are reserved against both existing tasks and the pending import batch.

use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue;

use super::{ImportReport, OnDuplicate};

#[allow(clippy::too_many_arguments)]
pub(super) fn merge_imported_tasks(
    queue: &mut QueueFile,
    done: Option<&QueueFile>,
    imported: Vec<Task>,
    id_prefix: &str,
    id_width: usize,
    max_depth: u8,
    now: &str,
    on_duplicate: OnDuplicate,
) -> Result<ImportReport> {
    let mut existing_ids: HashSet<String> =
        queue.tasks.iter().map(|task| task.id.clone()).collect();
    if let Some(done) = done {
        existing_ids.extend(done.tasks.iter().map(|task| task.id.clone()));
    }

    let mut report = ImportReport {
        parsed: imported.len(),
        ..ImportReport::default()
    };
    let mut tasks_to_add = Vec::new();
    let mut needs_new_id = Vec::new();

    struct NeedsId {
        idx: usize,
        old_id: Option<String>,
    }

    for mut task in imported {
        let has_id = !task.id.is_empty();
        if has_id {
            let is_duplicate = existing_ids.contains(&task.id)
                || tasks_to_add
                    .iter()
                    .any(|candidate: &Task| candidate.id == task.id);
            if is_duplicate {
                match on_duplicate {
                    OnDuplicate::Fail => {
                        bail!(
                            "Duplicate task ID detected: '{}'. Use --on-duplicate skip or rename to handle duplicates.",
                            task.id
                        );
                    }
                    OnDuplicate::Skip => {
                        report.skipped_duplicates += 1;
                        continue;
                    }
                    OnDuplicate::Rename => {
                        let old_id = task.id.clone();
                        task.id.clear();
                        needs_new_id.push(NeedsId {
                            idx: tasks_to_add.len(),
                            old_id: Some(old_id),
                        });
                        tasks_to_add.push(task);
                        continue;
                    }
                }
            }
        } else {
            needs_new_id.push(NeedsId {
                idx: tasks_to_add.len(),
                old_id: None,
            });
        }

        tasks_to_add.push(task);
    }

    if !needs_new_id.is_empty() {
        let mut temp_queue = queue.clone();
        for need in &needs_new_id {
            let new_id = queue::next_id_across(&temp_queue, done, id_prefix, id_width, max_depth)?;
            if let Some(old_id) = need.old_id.as_ref() {
                report
                    .rename_mappings
                    .push((old_id.clone(), new_id.clone()));
            }
            if let Some(task) = tasks_to_add.get_mut(need.idx) {
                task.id = new_id.clone();
            }
            temp_queue.tasks.push(create_placeholder_task(new_id, now));
        }
    }

    report.renamed = report.rename_mappings.len();
    let new_task_ids: Vec<String> = tasks_to_add.iter().map(|task| task.id.clone()).collect();
    report.imported = new_task_ids.len();

    queue.tasks.extend(tasks_to_add);
    if !new_task_ids.is_empty() {
        let insert_at = crate::queue::operations::suggest_new_task_insert_index(queue);
        crate::queue::operations::reposition_new_tasks(queue, &new_task_ids, insert_at);
    }

    Ok(report)
}

fn create_placeholder_task(id: String, now: &str) -> Task {
    Task {
        id,
        title: "__import_id_reservation__".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: TaskPriority::Medium,
        created_at: Some(now.to_string()),
        updated_at: Some(now.to_string()),
        ..Default::default()
    }
}
