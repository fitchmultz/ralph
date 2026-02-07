//! Task selection helpers for run commands.
//!
//! Responsibilities:
//! - Apply `run one`/`run --target` selection rules over queue and done data.
//! - Delegate to queue selection helpers while preserving CLI operation context.
//!
//! Does not handle:
//! - Persisting queue state or mutating tasks.
//! - Dependency resolution beyond what queue selection helpers enforce.
//!
//! Assumptions/invariants:
//! - Callers pass fully loaded `QueueFile` values and consistent done refs.
//! - Target task IDs are trimmed and validated by downstream helpers.

use crate::contracts::QueueFile;
use crate::queue::operations::{
    RunnableSelectionOptions, is_task_runnable, select_runnable_task_index,
    select_runnable_task_index_with_target,
};
use anyhow::Result;
use std::collections::HashSet;

pub(crate) fn select_run_one_task_index(
    queue_file: &QueueFile,
    done_ref: Option<&QueueFile>,
    target_task_id: Option<&str>,
    include_draft: bool,
) -> Result<Option<usize>> {
    let options = RunnableSelectionOptions::new(include_draft, true);
    if let Some(task_id) = target_task_id {
        return select_runnable_task_index_with_target(
            queue_file,
            done_ref,
            task_id,
            "run --target",
            options,
        )
        .map(Some);
    }

    Ok(select_runnable_task_index(queue_file, done_ref, options))
}

pub(crate) fn select_run_one_task_index_excluding(
    queue_file: &QueueFile,
    done_ref: Option<&QueueFile>,
    include_draft: bool,
    in_flight: &HashSet<String>,
) -> Result<Option<usize>> {
    let options = RunnableSelectionOptions::new(include_draft, true);
    let is_in_flight = |id: &str| in_flight.contains(id.trim());

    if options.prefer_doing
        && let Some(idx) = queue_file.tasks.iter().position(|task| {
            task.status == crate::contracts::TaskStatus::Doing && !is_in_flight(&task.id)
        })
    {
        return Ok(Some(idx));
    }

    if let Some(idx) = queue_file.tasks.iter().position(|task| {
        task.status == crate::contracts::TaskStatus::Todo
            && !is_in_flight(&task.id)
            && is_task_runnable(task, queue_file, done_ref)
    }) {
        return Ok(Some(idx));
    }

    if options.include_draft {
        return Ok(queue_file.tasks.iter().position(|task| {
            task.status == crate::contracts::TaskStatus::Draft
                && !is_in_flight(&task.id)
                && is_task_runnable(task, queue_file, done_ref)
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{select_run_one_task_index, select_run_one_task_index_excluding};
    use crate::contracts::{QueueFile, Task, TaskStatus};
    use crate::queue::operations::QueueQueryError;
    use std::collections::HashSet;

    fn task_with_status(status: TaskStatus) -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
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
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            parent_id: None,
        }
    }

    fn task_with_id_status(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
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
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            parent_id: None,
        }
    }

    fn base_task() -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
            description: None,
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
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: std::collections::HashMap::new(),
            parent_id: None,
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
        assert!(
            matches!(
                err.downcast_ref::<QueueQueryError>(),
                Some(QueueQueryError::TargetTaskNotFound { .. })
            ),
            "expected TargetTaskNotFound error"
        );
    }

    #[test]
    fn select_run_one_task_index_errors_when_target_done() {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Done)]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)
            .expect_err("done target should error");
        assert!(
            matches!(
                err.downcast_ref::<QueueQueryError>(),
                Some(QueueQueryError::TargetTaskNotRunnable {
                    status: TaskStatus::Done,
                    ..
                })
            ),
            "expected TargetTaskNotRunnable with Done status"
        );
    }

    #[test]
    fn select_run_one_task_index_errors_when_target_rejected() {
        let queue_file = queue_with_tasks(vec![task_with_status(TaskStatus::Rejected)]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)
            .expect_err("rejected target should error");
        assert!(
            matches!(
                err.downcast_ref::<QueueQueryError>(),
                Some(QueueQueryError::TargetTaskNotRunnable {
                    status: TaskStatus::Rejected,
                    ..
                })
            ),
            "expected TargetTaskNotRunnable with Rejected status"
        );
    }

    #[test]
    fn select_run_one_task_index_errors_when_dependencies_unmet() {
        let mut task = base_task();
        task.depends_on = vec!["RQ-0002".to_string()];
        let queue_file = queue_with_tasks(vec![task]);
        let err = select_run_one_task_index(&queue_file, None, Some("RQ-0001"), false)
            .expect_err("blocked target should error");
        assert!(
            matches!(
                err.downcast_ref::<QueueQueryError>(),
                Some(QueueQueryError::TargetTaskBlockedByUnmetDependencies { .. })
            ),
            "expected TargetTaskBlockedByUnmetDependencies error"
        );
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
        assert!(
            matches!(
                err.downcast_ref::<QueueQueryError>(),
                Some(QueueQueryError::TargetTaskDraftExcluded { .. })
            ),
            "expected TargetTaskDraftExcluded error"
        );
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
    fn select_run_one_task_index_excluding_skips_in_flight() -> anyhow::Result<()> {
        let queue_file = queue_with_tasks(vec![
            task_with_id_status("RQ-0001", TaskStatus::Todo),
            task_with_id_status("RQ-0002", TaskStatus::Todo),
        ]);
        let mut in_flight = HashSet::new();
        in_flight.insert("RQ-0001".to_string());

        let idx = select_run_one_task_index_excluding(&queue_file, None, false, &in_flight)?;
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
