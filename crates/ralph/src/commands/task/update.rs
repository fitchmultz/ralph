//! Task updating functionality for modifying existing tasks via runner invocation.
//!
//! Responsibilities:
//! - Update tasks using AI runners via .ralph/prompts/task_updater.md.
//! - Support single task updates and batch updates (update-all).
//! - Create queue backups before updates for recovery.
//! - Validate queue state before and after runner execution.
//! - Handle dry-run mode for previewing updates.
//! - Detect and report changed fields after updates.
//! - Handle tasks moved to done.json during updates.
//!
//! Not handled here:
//! - Task building (see build.rs).
//! - Refactor task generation (see refactor.rs).
//! - CLI argument parsing or command routing.
//! - Direct queue file manipulation outside of runner-driven changes.
//!
//! Invariants/assumptions:
//! - Queue file is the source of truth for task state.
//! - Runner execution produces valid task JSON output.
//! - Backup is created before any mutations for recovery.
//! - Lock acquisition is optional (controlled by acquire_lock parameter).
//! - Tasks may be moved to done.json during updates (not an error).

use super::{TaskUpdateSettings, compare_task_fields, resolve_task_update_settings};
use crate::commands::run::PhaseType;
use crate::contracts::{ProjectType, QueueFile};
use crate::{config, fsutil, prompts, queue, runner, runutil};
use anyhow::{Context, Result, anyhow, bail};
use std::path::Path;

pub fn update_task(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<()> {
    update_task_impl(resolved, task_id, settings, true)
}

pub fn update_task_without_lock(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
) -> Result<()> {
    update_task_impl(resolved, task_id, settings, false)
}

pub fn update_all_tasks(resolved: &config::Resolved, settings: &TaskUpdateSettings) -> Result<()> {
    let _queue_lock =
        queue::acquire_queue_lock(&resolved.repo_root, "task update", settings.force)?;

    let queue_file = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    if queue_file.tasks.is_empty() {
        bail!("No tasks in queue to update.");
    }

    let task_ids: Vec<String> = queue_file
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect();
    for task_id in task_ids {
        update_task_impl(resolved, &task_id, settings, false)?;
    }

    Ok(())
}

/// Restore queue file from backup.
/// Returns Ok(()) on successful restore, Err otherwise.
fn restore_queue_from_backup(queue_path: &Path, backup_path: &Path) -> Result<()> {
    let bytes = std::fs::read(backup_path)
        .with_context(|| format!("read queue backup {}", backup_path.display()))?;
    fsutil::write_atomic(queue_path, &bytes)
        .with_context(|| format!("restore queue from backup {}", backup_path.display()))?;
    Ok(())
}

/// Load, validate, and save queue after task update with automatic backup restoration on failure.
///
/// This function attempts to:
/// 1. Load the queue after the runner has modified it
/// 2. Validate the queue against semantic rules
/// 3. Save the normalized queue back to disk
///
/// If any of these steps fail, the original queue is automatically restored from backup
/// and the error is returned with context about the restoration.
fn load_validate_and_save_queue_after_update(
    resolved: &config::Resolved,
    backup_path: &Path,
    max_depth: u8,
) -> Result<QueueFile> {
    // Step 1: Load queue after update (with repair for common JSON errors)
    let after = queue::load_queue_with_repair(&resolved.queue_path)
        .with_context(|| "parse queue after task update")
        .or_else(
            |err| match restore_queue_from_backup(&resolved.queue_path, backup_path) {
                Ok(()) => Err(err).with_context(|| {
                    format!(
                        "queue parse failed after task update; restored queue from backup {}",
                        backup_path.display()
                    )
                }),
                Err(restore_err) => Err(err).with_context(|| {
                    format!(
                        "queue parse failed after task update AND restore failed (backup {}): {:#}",
                        backup_path.display(),
                        restore_err
                    )
                }),
            },
        )?;

    // Step 2: Prepare done file reference for validation
    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_after_ref = if done_after.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_after)
    };

    // Step 3: Validate queue set (semantic validation)
    queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set after task update")
    .or_else(|err| {
        match restore_queue_from_backup(&resolved.queue_path, backup_path) {
            Ok(()) => Err(err).with_context(|| {
                format!(
                    "queue validation failed after task update; restored queue from backup {}",
                    backup_path.display()
                )
            }),
            Err(restore_err) => Err(err).with_context(|| {
                format!(
                    "queue validation failed after task update AND restore failed (backup {}): {:#}",
                    backup_path.display(),
                    restore_err
                )
            }),
        }
    })?;

    // Step 4: Save the validated queue
    queue::save_queue(&resolved.queue_path, &after)
        .context("save queue after task update")
        .or_else(
            |err| match restore_queue_from_backup(&resolved.queue_path, backup_path) {
                Ok(()) => Err(err).with_context(|| {
                    format!(
                        "queue save failed after task update; restored queue from backup {}",
                        backup_path.display()
                    )
                }),
                Err(restore_err) => Err(err).with_context(|| {
                    format!(
                        "queue save failed after task update AND restore failed (backup {}): {:#}",
                        backup_path.display(),
                        restore_err
                    )
                }),
            },
        )?;

    Ok(after)
}

fn update_task_impl(
    resolved: &config::Resolved,
    task_id: &str,
    settings: &TaskUpdateSettings,
    acquire_lock: bool,
) -> Result<()> {
    // Handle dry-run mode early (before any mutations)
    if settings.dry_run {
        let before = queue::load_queue(&resolved.queue_path)
            .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

        let task_id = task_id.trim();
        let task = before
            .tasks
            .iter()
            .find(|t| t.id.trim() == task_id)
            .ok_or_else(|| anyhow!("{}", crate::error_messages::task_not_found(task_id)))?;

        let template = prompts::load_task_updater_prompt(&resolved.repo_root)?;
        let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
        let prompt = prompts::render_task_updater_prompt(
            &template,
            task_id,
            project_type,
            &resolved.config,
        )?;

        println!("Dry run - would update task {}:", task_id);
        println!("  Current title: {}", task.title);
        println!("\n  Prompt preview (first 800 chars):");
        let preview_len = prompt.len().min(800);
        println!("{}", &prompt[..preview_len]);
        if prompt.len() > 800 {
            println!("\n  ... ({} more characters)", prompt.len() - 800);
        }
        println!("\n  Note: Actual changes depend on runner analysis of repository state.");
        return Ok(());
    }

    let _queue_lock = if acquire_lock {
        Some(queue::acquire_queue_lock(
            &resolved.repo_root,
            "task update",
            settings.force,
        )?)
    } else {
        None
    };

    // Create backup before running task updater
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let backup_path = queue::backup_queue(&resolved.queue_path, &cache_dir)
        .with_context(|| "failed to create queue backup before task update")?;
    log::debug!("Created queue backup at: {}", backup_path.display());

    let before = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("read queue {}", resolved.queue_path.display()))?;

    let task_id = task_id.trim();
    if !before.tasks.iter().any(|t| t.id.trim() == task_id) {
        bail!("{}", crate::error_messages::task_not_found(task_id));
    }

    let before_task = before
        .tasks
        .iter()
        .find(|t| t.id.trim() == task_id)
        .unwrap();
    let before_json = serde_json::to_string(before_task)?;

    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &before,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set before task update")?;

    let template = prompts::load_task_updater_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt =
        prompts::render_task_updater_prompt(&template, task_id, project_type, &resolved.config)?;

    let prompt =
        prompts::wrap_with_repoprompt_requirement(&prompt, settings.repoprompt_tool_injection);
    let prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    let runner_settings = resolve_task_update_settings(resolved, settings)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    let retry_policy = runutil::RunnerRetryPolicy::from_config(&resolved.config.agent.runner_retry)
        .unwrap_or_default();

    let _output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: runner_settings.runner,
            bins,
            model: runner_settings.model.clone(),
            reasoning_effort: runner_settings.reasoning_effort,
            runner_cli: runner_settings.runner_cli,
            prompt: &prompt,
            timeout: None,
            permission_mode: runner_settings.permission_mode,
            revert_on_error: true,
            git_revert_mode: resolved
                .config
                .agent
                .git_revert_mode
                .unwrap_or(crate::contracts::GitRevertMode::Ask),
            output_handler: None,
            output_stream: runner::OutputStream::Terminal,
            revert_prompt: None,
            phase_type: PhaseType::SinglePhase,
            session_id: None,
            retry_policy,
        },
        runutil::RunnerErrorMessages {
            log_label: "task updater",
            interrupted_msg: "Task updater interrupted: agent run was canceled.",
            timeout_msg: "Task updater timed out: agent run exceeded time limit. Changes in the working tree were reverted; review repo state manually.",
            terminated_msg: "Task updater terminated: agent was stopped by a signal. Review uncommitted changes before rerunning.",
            non_zero_msg: |code| {
                format!(
                    "Task updater failed: agent exited with a non-zero code ({}). Changes in the working tree were reverted; review repo state before rerunning.",
                    code
                )
            },
            other_msg: |err| {
                format!(
                    "Task updater failed: agent could not be started or encountered an error. Error: {:#}",
                    err
                )
            },
        },
    )?;

    // Load, validate, and save queue after update with automatic backup restoration on failure
    let after = load_validate_and_save_queue_after_update(resolved, &backup_path, max_depth)?;

    // Load done_after again since it may have been modified during update
    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;

    // Look up the task after update - it may have been moved to done.json or removed
    match after.tasks.iter().find(|t| t.id.trim() == task_id) {
        Some(after_task) => {
            let after_json = serde_json::to_string(after_task)?;

            if before_json == after_json {
                log::info!("Task {} updated. No changes detected.", task_id);
            } else {
                let changed_fields = compare_task_fields(&before_json, &after_json)?;
                log::info!(
                    "Task {} updated. Changed fields: {}",
                    task_id,
                    changed_fields.join(", ")
                );
            }
        }
        None => {
            // Task not in queue after update - check if it was moved to done.json
            match done_after.tasks.iter().find(|t| t.id.trim() == task_id) {
                Some(done_task) => {
                    let after_json = serde_json::to_string(done_task)?;

                    if before_json == after_json {
                        log::info!("Task {} moved to done.json. No changes detected.", task_id);
                    } else {
                        let changed_fields = compare_task_fields(&before_json, &after_json)?;
                        log::info!(
                            "Task {} moved to done.json. Changed fields: {}",
                            task_id,
                            changed_fields.join(", ")
                        );
                    }
                }
                None => {
                    log::warn!(
                        "Task {} was removed during update and not found in done.json.",
                        task_id
                    );
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Resolved;
    use crate::contracts::{Config, QueueFile, Task, TaskStatus};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn task_with_timestamps(
        id: &str,
        status: TaskStatus,
        created_at: Option<&str>,
        updated_at: Option<&str>,
    ) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec!["tag".to_string()],
            scope: vec!["file".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: created_at.map(|s| s.to_string()),
            updated_at: updated_at.map(|s| s.to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: HashMap::new(),
            estimated_minutes: None,
            actual_minutes: None,
            parent_id: None,
        }
    }

    fn create_test_resolved(temp: &TempDir) -> Result<Resolved> {
        let repo_root = temp.path().to_path_buf();
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir)?;

        Ok(Resolved {
            config: Config::default(),
            repo_root,
            queue_path: ralph_dir.join("queue.json"),
            done_path: ralph_dir.join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        })
    }

    #[test]
    fn restore_queue_from_backup_success() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");
        let backup_path = temp.path().join("queue.json.backup");

        // Create original queue
        let original = QueueFile {
            version: 1,
            tasks: vec![task_with_timestamps(
                "RQ-0001",
                TaskStatus::Todo,
                Some("2026-01-18T00:00:00Z"),
                Some("2026-01-18T00:00:00Z"),
            )],
        };
        queue::save_queue(&queue_path, &original)?;

        // Create backup
        queue::save_queue(&backup_path, &original)?;

        // Corrupt the queue
        std::fs::write(&queue_path, "corrupted json")?;

        // Restore from backup
        restore_queue_from_backup(&queue_path, &backup_path)?;

        // Verify restored
        let restored = queue::load_queue(&queue_path)?;
        assert_eq!(restored.tasks.len(), 1);
        assert_eq!(restored.tasks[0].id, "RQ-0001");

        Ok(())
    }

    #[test]
    fn load_validate_and_save_queue_restores_on_parse_failure() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = create_test_resolved(&temp)?;

        // Create valid initial queue with all required fields
        let initial = QueueFile {
            version: 1,
            tasks: vec![task_with_timestamps(
                "RQ-0001",
                TaskStatus::Todo,
                Some("2026-01-18T00:00:00Z"),
                Some("2026-01-18T00:00:00Z"),
            )],
        };
        queue::save_queue(&resolved.queue_path, &initial)?;

        // Create backup
        let backup_dir = resolved.repo_root.join(".ralph/cache");
        let backup_path = queue::backup_queue(&resolved.queue_path, &backup_dir)?;

        // Corrupt the queue with invalid JSON
        std::fs::write(&resolved.queue_path, "{ not valid json }")?;

        // Attempt to load/validate/save - should fail and restore backup
        let result = load_validate_and_save_queue_after_update(&resolved, &backup_path, 10);

        // Should return error
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("restored queue from backup"),
            "Error should mention backup restoration: {}",
            err_msg
        );

        // Verify queue was restored to backup content
        let restored_content = std::fs::read_to_string(&resolved.queue_path)?;
        let restored: QueueFile = serde_json::from_str(&restored_content)?;
        assert_eq!(restored.tasks.len(), 1);
        assert_eq!(restored.tasks[0].id, "RQ-0001");

        Ok(())
    }

    #[test]
    fn load_validate_and_save_queue_restores_on_validation_failure() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = create_test_resolved(&temp)?;

        // Create valid initial queue with all required fields
        let initial = QueueFile {
            version: 1,
            tasks: vec![task_with_timestamps(
                "RQ-0001",
                TaskStatus::Todo,
                Some("2026-01-18T00:00:00Z"),
                Some("2026-01-18T00:00:00Z"),
            )],
        };
        queue::save_queue(&resolved.queue_path, &initial)?;

        // Create backup
        let backup_dir = resolved.repo_root.join(".ralph/cache");
        let backup_path = queue::backup_queue(&resolved.queue_path, &backup_dir)?;

        // Replace queue with JSON that parses but fails semantic validation
        // (missing required timestamps)
        std::fs::write(
            &resolved.queue_path,
            r#"{"version":1,"tasks":[{"id":"RQ-0001","title":"Test","status":"todo","tags":[],"scope":[],"evidence":[],"plan":[],"notes":[],"depends_on":[],"blocks":[],"relates_to":[],"custom_fields":{}}]}"#,
        )?;

        // Attempt to load/validate/save - should fail and restore backup
        let result = load_validate_and_save_queue_after_update(&resolved, &backup_path, 10);

        // Should return error
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("restored queue from backup"),
            "Error should mention backup restoration: {}",
            err_msg
        );

        // Verify queue was restored to backup content
        let restored_content = std::fs::read_to_string(&resolved.queue_path)?;
        let restored: QueueFile = serde_json::from_str(&restored_content)?;
        assert_eq!(restored.tasks.len(), 1);
        assert_eq!(restored.tasks[0].id, "RQ-0001");

        Ok(())
    }

    #[test]
    fn load_validate_and_save_queue_succeeds_with_valid_queue() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = create_test_resolved(&temp)?;

        // Create valid initial queue
        let initial = QueueFile {
            version: 1,
            tasks: vec![task_with_timestamps(
                "RQ-0001",
                TaskStatus::Todo,
                Some("2026-01-18T00:00:00Z"),
                Some("2026-01-18T00:00:00Z"),
            )],
        };
        queue::save_queue(&resolved.queue_path, &initial)?;

        // Create backup
        let backup_dir = resolved.repo_root.join(".ralph/cache");
        let backup_path = queue::backup_queue(&resolved.queue_path, &backup_dir)?;

        // Replace queue with another valid queue (simulating a successful update)
        let updated = QueueFile {
            version: 1,
            tasks: vec![{
                let mut t = task_with_timestamps(
                    "RQ-0001",
                    TaskStatus::Todo,
                    Some("2026-01-18T00:00:00Z"),
                    Some("2026-01-19T00:00:00Z"), // updated timestamp
                );
                t.title = "Updated title".to_string();
                t
            }],
        };
        queue::save_queue(&resolved.queue_path, &updated)?;

        // Should succeed
        let result = load_validate_and_save_queue_after_update(&resolved, &backup_path, 10);
        assert!(result.is_ok());

        // Verify the updated content is preserved
        let final_content = std::fs::read_to_string(&resolved.queue_path)?;
        let final_queue: QueueFile = serde_json::from_str(&final_content)?;
        assert_eq!(final_queue.tasks.len(), 1);
        assert_eq!(final_queue.tasks[0].title, "Updated title");

        Ok(())
    }
}
