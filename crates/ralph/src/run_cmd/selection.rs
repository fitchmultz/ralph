//! Task selection helpers for run commands.
//!
//! Encapsulates selection rules for choosing which task to run during `run one`.

use crate::contracts::{QueueFile, TaskStatus};
use crate::queue;
use anyhow::{anyhow, bail, Result};

pub(crate) fn select_run_one_task_index(
    queue_file: &QueueFile,
    done_ref: Option<&QueueFile>,
    target_task_id: Option<&str>,
    include_draft: bool,
) -> Result<Option<usize>> {
    if let Some(task_id) = target_task_id {
        let needle = task_id.trim();
        if needle.is_empty() {
            bail!("Target task id is empty");
        }
        let idx = queue_file
            .tasks
            .iter()
            .position(|t| t.id.trim() == needle)
            .ok_or_else(|| anyhow!("Target task {} not found in queue", needle))?;
        let task = &queue_file.tasks[idx];
        match task.status {
            TaskStatus::Done | TaskStatus::Rejected => {
                bail!(
                    "Target task {} is not runnable (status: {}). Choose a todo/doing task.",
                    needle,
                    task.status
                );
            }
            TaskStatus::Draft => {
                if !include_draft {
                    bail!(
                        "Target task {} is in draft status. Use --include-draft to run draft tasks.",
                        needle
                    );
                }
                if !queue::are_dependencies_met(task, queue_file, done_ref) {
                    bail!(
                        "Target task {} is blocked by unmet dependencies. Resolve dependencies before running.",
                        needle
                    );
                }
            }
            TaskStatus::Todo => {
                if !queue::are_dependencies_met(task, queue_file, done_ref) {
                    bail!(
                        "Target task {} is blocked by unmet dependencies. Resolve dependencies before running.",
                        needle
                    );
                }
            }
            TaskStatus::Doing => {}
        }
        return Ok(Some(idx));
    }

    if let Some(idx) = queue_file
        .tasks
        .iter()
        .position(|t| t.status == TaskStatus::Doing)
    {
        return Ok(Some(idx));
    }

    Ok(queue_file.tasks.iter().position(|t| {
        if t.status == TaskStatus::Todo {
            return queue::are_dependencies_met(t, queue_file, done_ref);
        }
        if include_draft && t.status == TaskStatus::Draft {
            return queue::are_dependencies_met(t, queue_file, done_ref);
        }
        false
    }))
}

#[cfg(test)]
mod tests {
    use super::select_run_one_task_index;
    use crate::contracts::{QueueFile, Task, TaskStatus};

    fn task_with_status(status: TaskStatus) -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags: vec!["rust".to_string()],
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
            custom_fields: std::collections::HashMap::new(),
        }
    }

    fn base_task() -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            priority: Default::default(),
            tags: vec!["rust".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
        }
    }

    fn queue_with_tasks(tasks: Vec<Task>) -> QueueFile {
        QueueFile { version: 1, tasks }
    }

    #[test]
    fn select_run_one_task_index_finds_target() -> anyhow::Result<()> {
        let queue_file = queue_with_tasks(vec![base_task()]);
        let idx = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)?;
        assert_eq!(idx, Some(0));
        Ok(())
    }

    #[test]
    fn select_run_one_task_index_errors_when_target_missing() {
        let queue_file = queue_with_tasks(vec![base_task()]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-9999"), false)
            .expect_err("missing target should error");
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn select_run_one_task_index_errors_when_target_done() {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Done)]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)
            .expect_err("done target should error");
        assert!(err.to_string().contains("not runnable"));
    }

    #[test]
    fn select_run_one_task_index_errors_when_target_rejected() {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Rejected)]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)
            .expect_err("rejected target should error");
        assert!(err.to_string().contains("not runnable"));
    }

    #[test]
    fn select_run_one_task_index_errors_when_dependencies_unmet() {
        let mut task = base_task();
        task.depends_on = vec!["RQ-0002".to_string()];
        let queue_file = queue_with_tasks(vec![task]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)
            .expect_err("blocked target should error");
        assert!(err.to_string().contains("blocked by unmet dependencies"));
    }

    #[test]
    fn select_run_one_task_index_allows_doing() -> anyhow::Result<()> {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Doing)]);
        let idx = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)?;
        assert_eq!(idx, Some(0));
        Ok(())
    }

    #[test]
    fn select_run_one_task_index_rejects_draft_without_flag() {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Draft)]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)
            .expect_err("draft target should error");
        assert!(err.to_string().contains("draft"));
    }

    #[test]
    fn select_run_one_task_index_allows_draft_with_flag() -> anyhow::Result<()> {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Draft)]);
        let idx = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), true)?;
        assert_eq!(idx, Some(0));
        Ok(())
    }

    #[test]
    fn select_run_one_task_index_selects_draft_when_included() -> anyhow::Result<()> {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Draft)]);
        let idx = select_run_one_task_index(&queue_file, None, None, true)?;
        assert_eq!(idx, Some(0));
        Ok(())
    }
}
