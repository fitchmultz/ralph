//! Task selection helpers for run commands.
//!
//! Encapsulates selection rules for choosing which task to run during `run one`.

use crate::contracts::QueueFile;
use crate::queue::operations::{
    select_runnable_task_index, select_runnable_task_index_with_target, RunnableSelectionOptions,
};
use anyhow::Result;

pub(crate) fn select_run_one_task_index(
    queue_file: &QueueFile,
    done_ref: Option<&QueueFile>,
    target_task_id: Option<&str>,
    include_draft: bool,
) -> Result<Option<usize>> {
    let options = RunnableSelectionOptions::new(include_draft, true);
    if let Some(task_id) = target_task_id {
        return select_runnable_task_index_with_target(queue_file, done_ref, task_id, options)
            .map(Some);
    }

    Ok(select_runnable_task_index(queue_file, done_ref, options))
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

    fn task_with_id_status(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
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

    #[test]
    fn select_run_one_task_index_prefers_doing_over_todo() -> anyhow::Result<()> {
        let queue_file = queue_with_tasks(vec![
            task_with_id_status("RQ-0001", TaskStatus::Todo),
            task_with_id_status("RQ-0002", TaskStatus::Doing),
        ]);
        let idx = select_run_one_task_index(&queue_file, None, None, false)?;
        assert_eq!(idx, Some(1));
        Ok(())
    }

    #[test]
    fn select_run_one_task_index_prefers_todo_over_draft() -> anyhow::Result<()> {
        let queue_file = queue_with_tasks(vec![
            task_with_id_status("RQ-0001", TaskStatus::Draft),
            task_with_id_status("RQ-0002", TaskStatus::Todo),
        ]);
        let idx = select_run_one_task_index(&queue_file, None, None, true)?;
        assert_eq!(idx, Some(1));
        Ok(())
    }

    #[test]
    fn select_run_one_task_index_allows_rejected_dependency() -> anyhow::Result<()> {
        let mut task = base_task();
        task.depends_on = vec!["RQ-0002".to_string()];

        let mut dep = task_with_id_status("RQ-0002", TaskStatus::Rejected);
        dep.completed_at = Some("2026-01-18T00:00:00Z".to_string());

        let queue_file = queue_with_tasks(vec![task]);
        let done_file = queue_with_tasks(vec![dep]);

        let idx = select_run_one_task_index(&queue_file, Some(&done_file), Some("RQ-0001"), false)?;
        assert_eq!(idx, Some(0));
        Ok(())
    }
}
