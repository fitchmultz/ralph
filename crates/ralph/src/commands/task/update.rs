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
use crate::contracts::ProjectType;
use crate::{config, prompts, queue, runner, runutil};
use anyhow::{Context, Result, anyhow, bail};

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
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

        let template = prompts::load_task_updater_prompt(&resolved.repo_root)?;
        let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
        let prompt = prompts::render_task_updater_prompt(
            &template,
            task_id,
            &settings.fields,
            project_type,
            &resolved.config,
        )?;

        println!("Dry run - would update task {}:", task_id);
        println!("  Fields to update: {}", settings.fields);
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
        bail!("Task not found: {}", task_id);
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
    let prompt = prompts::render_task_updater_prompt(
        &template,
        task_id,
        &settings.fields,
        project_type,
        &resolved.config,
    )?;

    let prompt =
        prompts::wrap_with_repoprompt_requirement(&prompt, settings.repoprompt_tool_injection);
    let prompt =
        prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)?;

    let runner_settings = resolve_task_update_settings(resolved, settings)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

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

    // Load queue after update, with repair for common JSON errors
    let after = match queue::load_queue_with_repair(&resolved.queue_path) {
        Ok(queue) => queue,
        Err(err) => {
            log::error!(
                "Failed to parse queue after task update. Backup available at: {}",
                backup_path.display()
            );
            log::error!(
                "To restore from backup, copy the backup file to: {}",
                resolved.queue_path.display()
            );
            return Err(err).with_context(|| {
                format!(
                    "task update for {}: queue file may be corrupted. Backup: {}",
                    task_id,
                    backup_path.display()
                )
            });
        }
    };

    let done_after = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("read done {}", resolved.done_path.display()))?;
    let done_after_ref = if done_after.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_after)
    };
    queue::validate_queue_set(
        &after,
        done_after_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )
    .context("validate queue set after task update")?;

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

    queue::save_queue(&resolved.queue_path, &after).context("save queue after task update")?;

    Ok(())
}
