//! Queue maintenance operations for post-run supervision.
//!
//! Responsibilities:
//! - Explicitly repair and validate queue and done files for post-run recovery.
//! - Ensure task status transitions to Done appropriately.
//! - Handle queue/done file persistence.
//!
//! Not handled here:
//! - Git operations (see git_ops.rs).
//! - CI gate execution (see ci.rs).
//! - Notification logic (see notify.rs).
//!
//! Invariants/assumptions:
//! - Queue files follow the QueueFile schema.
//! - Terminal tasks have completed_at backfilled if missing.
//! - Task IDs are unique across queue and done files.

use crate::contracts::{QueueFile, TaskStatus};
use crate::runutil;
use crate::{queue, timeutil};
use anyhow::{Result, anyhow, bail};

/// Explicitly repairs timestamp maintenance, persists it, and validates the queue and done files.
pub(crate) fn maintain_and_validate_queues(
    resolved: &crate::config::Resolved,
) -> Result<(QueueFile, QueueFile)> {
    let (queue_file, done_file_opt) = queue::repair_and_validate_queues(resolved, true)?;
    let done_file = done_file_opt.unwrap_or_default();

    Ok((queue_file, done_file))
}

/// Returns the status and title of a task, or an error if not found.
pub(crate) fn require_task_status(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Result<(TaskStatus, String, bool)> {
    find_task_status(queue_file, done_file, task_id).ok_or_else(|| {
        anyhow!(
            "{}",
            crate::error_messages::task_not_found_in_queue_or_done(task_id)
        )
    })
}

/// Finds a task's status, title, and whether it's in the done file.
pub(crate) fn find_task_status(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Option<(TaskStatus, String, bool)> {
    let needle = task_id.trim();
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), false));
    }
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == needle) {
        return Some((task.status, task.title.clone(), true));
    }
    None
}

/// Ensures a task is marked as Done when the repo is dirty, handling revert-mode on inconsistency.
pub(crate) fn ensure_task_done_dirty_or_revert(
    resolved: &crate::config::Resolved,
    queue_file: &mut QueueFile,
    task_id: &str,
    task_status: TaskStatus,
    in_done: bool,
    git_revert_mode: crate::contracts::GitRevertMode,
    revert_prompt: Option<&runutil::RevertPromptHandler>,
) -> Result<()> {
    if task_status != TaskStatus::Done {
        if in_done {
            let outcome = runutil::apply_git_revert_mode(
                &resolved.repo_root,
                git_revert_mode,
                "Task inconsistency detected",
                revert_prompt,
            )?;
            bail!(
                "{}",
                runutil::format_revert_failure_message(
                    &format!(
                        "Task inconsistency: task {task_id} is archived in .ralph/done.jsonc but its status is not 'done'. Review the task state in .ralph/done.jsonc."
                    ),
                    outcome,
                )
            );
        }
        let now = timeutil::now_utc_rfc3339()?;
        queue::set_status(queue_file, task_id, TaskStatus::Done, &now, None)?;
        queue::save_queue(&resolved.queue_path, queue_file)?;
    }
    Ok(())
}

/// Ensures a task is marked as Done when the repo is clean, bailing on inconsistency.
pub(crate) fn ensure_task_done_clean_or_bail(
    resolved: &crate::config::Resolved,
    queue_file: &mut QueueFile,
    task_id: &str,
    task_status: TaskStatus,
    in_done: bool,
) -> Result<bool> {
    if task_status != TaskStatus::Done {
        if in_done {
            bail!(
                "Task inconsistency: task {task_id} is archived in .ralph/done.jsonc but its status is not 'done'. Review the task state in .ralph/done.jsonc."
            );
        }
        let now = timeutil::now_utc_rfc3339()?;
        queue::set_status(queue_file, task_id, TaskStatus::Done, &now, None)?;
        queue::save_queue(&resolved.queue_path, queue_file)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        AgentConfig, Config, NotificationConfig, QueueConfig, QueueFile, Runner, RunnerRetryConfig,
        Task, TaskPriority, TaskStatus,
    };
    use crate::queue;
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_queue(repo_root: &Path, status: TaskStatus) -> Result<()> {
        let task = Task {
            id: "RQ-0001".to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec!["tests".to_string()],
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: None,
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
            estimated_minutes: None,
            actual_minutes: None,
            parent_id: None,
        };

        queue::save_queue(
            &repo_root.join(".ralph/queue.json"),
            &QueueFile {
                version: 1,
                tasks: vec![task],
            },
        )?;
        Ok(())
    }

    fn resolved_for_repo(repo_root: &Path) -> crate::config::Resolved {
        let cfg = Config {
            agent: AgentConfig {
                runner: Some(Runner::Codex),
                model: Some(crate::contracts::Model::Gpt52Codex),
                reasoning_effort: Some(crate::contracts::ReasoningEffort::Medium),
                iterations: Some(1),
                followup_reasoning_effort: None,
                codex_bin: Some("codex".to_string()),
                opencode_bin: Some("opencode".to_string()),
                gemini_bin: Some("gemini".to_string()),
                claude_bin: Some("claude".to_string()),
                cursor_bin: Some("agent".to_string()),
                kimi_bin: Some("kimi".to_string()),
                pi_bin: Some("pi".to_string()),
                claude_permission_mode: Some(
                    crate::contracts::ClaudePermissionMode::BypassPermissions,
                ),
                runner_cli: None,
                phase_overrides: None,
                instruction_files: None,
                repoprompt_plan_required: Some(false),
                repoprompt_tool_injection: Some(false),
                ci_gate: Some(crate::contracts::CiGateConfig {
                    enabled: Some(false),
                    argv: None,
                }),
                git_revert_mode: Some(crate::contracts::GitRevertMode::Disabled),
                git_commit_push_enabled: Some(true),
                phases: Some(2),
                notification: NotificationConfig {
                    enabled: Some(false),
                    ..NotificationConfig::default()
                },
                webhook: crate::contracts::WebhookConfig::default(),
                runner_retry: RunnerRetryConfig::default(),
                session_timeout_hours: None,
                scan_prompt_version: None,
            },
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.json")),
                done_file: Some(PathBuf::from(".ralph/done.json")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(4),
                size_warning_threshold_kb: Some(500),
                task_count_warning_threshold: Some(500),
                max_dependency_depth: Some(10),
                auto_archive_terminal_after_days: None,
                aging_thresholds: None,
            },
            ..Config::default()
        };

        crate::config::Resolved {
            config: cfg,
            repo_root: repo_root.to_path_buf(),
            queue_path: repo_root.join(".ralph/queue.json"),
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.json")),
        }
    }

    #[test]
    fn maintain_and_validate_queues_backfills_missing_completed_at() -> Result<()> {
        let temp = TempDir::new()?;
        write_queue(temp.path(), TaskStatus::Done)?;

        let resolved = resolved_for_repo(temp.path());
        let (queue_file, _done_file) = maintain_and_validate_queues(&resolved)?;

        // Task should be in queue (not archived yet in this test)
        let task = queue_file
            .tasks
            .iter()
            .find(|t| t.id == "RQ-0001")
            .expect("expected task in queue");
        let completed_at = task
            .completed_at
            .as_deref()
            .expect("completed_at should be stamped");

        crate::timeutil::parse_rfc3339(completed_at)?;

        Ok(())
    }

    #[test]
    fn find_task_status_finds_in_queue() -> Result<()> {
        let temp = TempDir::new()?;
        write_queue(temp.path(), TaskStatus::Todo)?;

        let queue_file = queue::load_queue(&temp.path().join(".ralph/queue.json"))?;
        let done_file = QueueFile::default();

        let (status, title, in_done) =
            find_task_status(&queue_file, &done_file, "RQ-0001").expect("should find task");

        assert_eq!(status, TaskStatus::Todo);
        assert_eq!(title, "Test task");
        assert!(!in_done);

        Ok(())
    }

    #[test]
    fn find_task_status_finds_in_done() -> Result<()> {
        let temp = TempDir::new()?;
        write_queue(temp.path(), TaskStatus::Done)?;

        let resolved = resolved_for_repo(temp.path());
        // Archive the task to done file
        queue::archive_terminal_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
            10,
        )?;

        let queue_file = queue::load_queue(&resolved.queue_path)?;
        let done_file = queue::load_queue_or_default(&resolved.done_path)?;

        let (status, title, in_done) =
            find_task_status(&queue_file, &done_file, "RQ-0001").expect("should find task");

        assert_eq!(status, TaskStatus::Done);
        assert_eq!(title, "Test task");
        assert!(in_done);

        Ok(())
    }

    #[test]
    fn find_task_status_returns_none_for_missing() {
        let queue_file = QueueFile::default();
        let done_file = QueueFile::default();

        let result = find_task_status(&queue_file, &done_file, "RQ-9999");
        assert!(result.is_none());
    }

    #[test]
    fn require_task_status_errors_for_missing() {
        let queue_file = QueueFile::default();
        let done_file = QueueFile::default();

        let err = require_task_status(&queue_file, &done_file, "RQ-9999").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn ensure_task_done_clean_or_bail_marks_done_when_needed() -> Result<()> {
        let temp = TempDir::new()?;
        write_queue(temp.path(), TaskStatus::Todo)?;

        let resolved = resolved_for_repo(temp.path());
        let mut queue_file = queue::load_queue(&resolved.queue_path)?;

        let changed = ensure_task_done_clean_or_bail(
            &resolved,
            &mut queue_file,
            "RQ-0001",
            TaskStatus::Todo,
            false,
        )?;

        assert!(changed);

        // Reload and verify
        let queue_file = queue::load_queue(&resolved.queue_path)?;
        let task = queue_file.tasks.iter().find(|t| t.id == "RQ-0001").unwrap();
        assert_eq!(task.status, TaskStatus::Done);

        Ok(())
    }

    #[test]
    fn ensure_task_done_clean_or_bail_no_change_when_already_done() -> Result<()> {
        let temp = TempDir::new()?;
        write_queue(temp.path(), TaskStatus::Done)?;

        let resolved = resolved_for_repo(temp.path());
        let mut queue_file = queue::load_queue(&resolved.queue_path)?;

        let changed = ensure_task_done_clean_or_bail(
            &resolved,
            &mut queue_file,
            "RQ-0001",
            TaskStatus::Done,
            false,
        )?;

        assert!(!changed);

        Ok(())
    }

    #[test]
    fn ensure_task_done_clean_or_bail_errors_on_inconsistency() -> Result<()> {
        let temp = TempDir::new()?;
        write_queue(temp.path(), TaskStatus::Todo)?;

        let resolved = resolved_for_repo(temp.path());
        let mut queue_file = queue::load_queue(&resolved.queue_path)?;

        let err = ensure_task_done_clean_or_bail(
            &resolved,
            &mut queue_file,
            "RQ-0001",
            TaskStatus::Todo,
            true, // in_done = true but status is Todo - inconsistency!
        )
        .unwrap_err();

        assert!(err.to_string().contains("inconsistency"));

        Ok(())
    }
}
