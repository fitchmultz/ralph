//! Post-run supervision helpers.
//!
//! Handles post-run CI gating, queue/done updates, and git push/commit logic.

use super::logging;
use super::PhaseType;
use crate::contracts::{GitRevertMode, QueueFile, TaskStatus};
use crate::git::GitError;
use crate::notification;
use crate::{git, outpututil, queue, runutil, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::Stdio;

#[derive(Clone)]
pub(crate) struct ContinueSession {
    pub runner: crate::contracts::Runner,
    pub model: crate::contracts::Model,
    pub reasoning_effort: Option<crate::contracts::ReasoningEffort>,
    pub session_id: Option<String>,
    pub output_handler: Option<crate::runner::OutputHandler>,
    pub output_stream: crate::runner::OutputStream,
    /// Number of automatic "fix CI and rerun" retries already sent for the current CI gate loop.
    /// Used to auto-enforce CI compliance without prompting for the first N failures.
    pub ci_failure_retry_count: u8,
}

pub(crate) fn resume_continue_session(
    resolved: &crate::config::Resolved,
    session: &mut ContinueSession,
    message: &str,
) -> Result<crate::runner::RunnerOutput> {
    let Some(session_id) = session.session_id.as_deref() else {
        bail!("Catastrophic: no session id captured; cannot Continue.");
    };
    let bins = crate::runner::resolve_binaries(&resolved.config.agent);
    let runner_cli = crate::runner::resolve_agent_settings(
        Some(session.runner),
        Some(session.model.clone()),
        session.reasoning_effort,
        &crate::contracts::RunnerCliOptionsPatch::default(),
        None,
        &resolved.config.agent,
    )?
    .runner_cli;
    let output = crate::runner::resume_session(
        session.runner,
        &resolved.repo_root,
        bins,
        session.model.clone(),
        session.reasoning_effort,
        runner_cli,
        session_id,
        message,
        resolved.config.agent.claude_permission_mode,
        None,
        session.output_handler.clone(),
        session.output_stream,
        PhaseType::Implementation,
    )?;
    if let Some(new_id) = output.session_id.as_ref() {
        session.session_id = Some(new_id.clone());
    }
    Ok(output)
}

#[derive(Debug, Clone, Copy)]
enum QueueMaintenanceSaveMode {
    /// Save both queue and done files if any terminal task was repaired (backfilled).
    SaveBothIfAnyRepaired,
    /// Save each file independently if it specifically was repaired.
    SaveEachIfRepaired,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn post_run_supervise(
    resolved: &crate::config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    notify_on_complete: Option<bool>,
    notify_sound: Option<bool>,
    lfs_check: bool,
) -> Result<()> {
    let label = format!("PostRunSupervise for {}", task_id.trim());
    logging::with_scope(&label, || {
        let status = git::status_porcelain(&resolved.repo_root)?;
        let is_dirty = !status.trim().is_empty();

        let (mut queue_file, mut done_file) =
            maintain_and_validate_queues(resolved, QueueMaintenanceSaveMode::SaveBothIfAnyRepaired)
                .context("Initial queue maintenance failed")?;

        let (mut task_status, task_title, mut in_done) =
            require_task_status(&queue_file, &done_file, task_id)?;

        if is_dirty {
            if let Err(err) = warn_if_modified_lfs(&resolved.repo_root, lfs_check) {
                return Err(anyhow!(
                    "LFS validation failed: {}. Use --lfs-check to enable strict validation or fix the LFS issues.",
                    err
                ));
            }
            if let Err(err) = run_ci_gate(resolved) {
                let outcome = runutil::apply_git_revert_mode(
                    &resolved.repo_root,
                    git_revert_mode,
                    "CI gate failure",
                    revert_prompt.as_ref(),
                )?;
                bail!(
                    "{} Error: {:#}",
                    runutil::format_revert_failure_message(
                        &format!(
                            "CI gate failed: '{}' did not pass after the task completed.",
                            ci_gate_command_label(resolved)
                        ),
                        outcome,
                    ),
                    err
                );
            }

            let (q, d) = maintain_and_validate_queues(
                resolved,
                QueueMaintenanceSaveMode::SaveEachIfRepaired,
            )
            .context("Post-CI queue maintenance failed")?;
            queue_file = q;
            done_file = d;

            let (status_after, _title_after, in_done_after) =
                require_task_status(&queue_file, &done_file, task_id)?;
            task_status = status_after;
            in_done = in_done_after;

            ensure_task_done_dirty_or_revert(
                resolved,
                &mut queue_file,
                task_id,
                task_status,
                in_done,
                git_revert_mode,
                revert_prompt.as_ref(),
            )
            .context("Ensuring task is marked Done (dirty repo) failed")?;

            let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
            queue::archive_terminal_tasks(
                &resolved.queue_path,
                &resolved.done_path,
                &resolved.id_prefix,
                resolved.id_width,
                max_depth,
            )
            .context("Queue archiving failed")?;

            finalize_git_state(resolved, task_id, &task_title, git_commit_push_enabled)
                .context("Git finalization failed")?;

            // Trigger completion notification on successful completion
            let notify_config =
                build_notification_config(resolved, notify_on_complete, notify_sound);
            notification::notify_task_complete(task_id, &task_title, &notify_config);

            return Ok(());
        }

        if task_status == TaskStatus::Done && in_done {
            if git_commit_push_enabled {
                push_if_ahead(&resolved.repo_root).context("Git push failed")?;
            } else {
                log::info!("Auto git commit/push disabled; skipping push.");
            }

            // Trigger completion notification on successful completion
            let notify_config =
                build_notification_config(resolved, notify_on_complete, notify_sound);
            notification::notify_task_complete(task_id, &task_title, &notify_config);

            return Ok(());
        }

        let mut changed = ensure_task_done_clean_or_bail(
            resolved,
            &mut queue_file,
            task_id,
            task_status,
            in_done,
        )
        .context("Ensuring task is marked Done (clean repo) failed")?;

        let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
        let report = queue::archive_terminal_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
            max_depth,
        )
        .context("Queue archiving failed")?;
        if !report.moved_ids.is_empty() {
            changed = true;
        }

        if !changed {
            return Ok(());
        }

        finalize_git_state(resolved, task_id, &task_title, git_commit_push_enabled)
            .context("Git finalization failed")?;

        // Trigger completion notification on successful completion
        let notify_config = build_notification_config(resolved, notify_on_complete, notify_sound);
        notification::notify_task_complete(task_id, &task_title, &notify_config);

        Ok(())
    })
}

/// Build notification configuration from resolved config and CLI overrides.
fn build_notification_config(
    resolved: &crate::config::Resolved,
    notify_on_complete: Option<bool>,
    notify_sound: Option<bool>,
) -> notification::NotificationConfig {
    // CLI overrides take precedence over config
    let enabled = notify_on_complete
        .or(resolved.config.agent.notification.enabled)
        .unwrap_or(true);
    let notify_on_complete = notify_on_complete
        .or(resolved.config.agent.notification.notify_on_complete)
        .unwrap_or(true);
    let notify_on_fail = resolved
        .config
        .agent
        .notification
        .notify_on_fail
        .unwrap_or(true);
    let notify_on_loop_complete = resolved
        .config
        .agent
        .notification
        .notify_on_loop_complete
        .unwrap_or(true);
    let suppress_when_active = resolved
        .config
        .agent
        .notification
        .suppress_when_active
        .unwrap_or(true);
    let sound_enabled = notify_sound
        .or(resolved.config.agent.notification.sound_enabled)
        .unwrap_or(false);
    notification::NotificationConfig {
        enabled,
        notify_on_complete,
        notify_on_fail,
        notify_on_loop_complete,
        suppress_when_active,
        sound_enabled,
        sound_path: resolved.config.agent.notification.sound_path.clone(),
        timeout_ms: resolved
            .config
            .agent
            .notification
            .timeout_ms
            .unwrap_or(8000),
    }
}

/// Loads, repairs (backfills completed_at), and validates the queue and done files.
fn maintain_and_validate_queues(
    resolved: &crate::config::Resolved,
    save_mode: QueueMaintenanceSaveMode,
) -> Result<(QueueFile, QueueFile)> {
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let mut done_file = queue::load_queue_or_default(&resolved.done_path)?;
    let repair_now = timeutil::now_utc_rfc3339()?;

    match save_mode {
        QueueMaintenanceSaveMode::SaveBothIfAnyRepaired => {
            let mut repaired = false;
            if queue::backfill_terminal_completed_at(&mut queue_file, &repair_now) > 0 {
                repaired = true;
            }
            if queue::backfill_terminal_completed_at(&mut done_file, &repair_now) > 0 {
                repaired = true;
            }
            if repaired {
                queue::save_queue(&resolved.queue_path, &queue_file)?;
                if !done_file.tasks.is_empty() || resolved.done_path.exists() {
                    queue::save_queue(&resolved.done_path, &done_file)?;
                }
            }
        }
        QueueMaintenanceSaveMode::SaveEachIfRepaired => {
            if queue::backfill_terminal_completed_at(&mut queue_file, &repair_now) > 0 {
                queue::save_queue(&resolved.queue_path, &queue_file)?;
            }
            if queue::backfill_terminal_completed_at(&mut done_file, &repair_now) > 0 {
                queue::save_queue(&resolved.done_path, &done_file)?;
            }
        }
    }

    let done_ref = if done_file.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done_file)
    };
    let max_depth = resolved.config.queue.max_dependency_depth.unwrap_or(10);
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
        max_depth,
    )?;

    Ok((queue_file, done_file))
}

/// Returns the status and title of a task, or an error if not found.
fn require_task_status(
    queue_file: &QueueFile,
    done_file: &QueueFile,
    task_id: &str,
) -> Result<(TaskStatus, String, bool)> {
    find_task_status(queue_file, done_file, task_id)
        .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))
}

/// Ensures a task is marked as Done when the repo is dirty, handling revert-mode on inconsistency.
fn ensure_task_done_dirty_or_revert(
    resolved: &crate::config::Resolved,
    queue_file: &mut QueueFile,
    task_id: &str,
    task_status: TaskStatus,
    in_done: bool,
    git_revert_mode: GitRevertMode,
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
                        "Task inconsistency: task {task_id} is archived in .ralph/done.json but its status is not 'done'. Review the task state in .ralph/done.json."
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
fn ensure_task_done_clean_or_bail(
    resolved: &crate::config::Resolved,
    queue_file: &mut QueueFile,
    task_id: &str,
    task_status: TaskStatus,
    in_done: bool,
) -> Result<bool> {
    if task_status != TaskStatus::Done {
        if in_done {
            bail!("Task inconsistency: task {task_id} is archived in .ralph/done.json but its status is not 'done'. Review the task state in .ralph/done.json.");
        }
        let now = timeutil::now_utc_rfc3339()?;
        queue::set_status(queue_file, task_id, TaskStatus::Done, &now, None)?;
        queue::save_queue(&resolved.queue_path, queue_file)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Handles the final git commit and push if enabled, and verifies the repo is clean.
fn finalize_git_state(
    resolved: &crate::config::Resolved,
    task_id: &str,
    task_title: &str,
    git_commit_push_enabled: bool,
) -> Result<()> {
    if git_commit_push_enabled {
        let commit_message = outpututil::format_task_commit_message(task_id, task_title);
        git::commit_all(&resolved.repo_root, &commit_message)?;
        push_if_ahead(&resolved.repo_root)?;
        git::require_clean_repo_ignoring_paths(
            &resolved.repo_root,
            false,
            git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
        )?;
    } else {
        log::info!("Auto git commit/push disabled; leaving repo dirty after queue updates.");
    }
    Ok(())
}

/// Validates LFS configuration and warns about potential issues.
///
/// When `strict` is true, returns an error if LFS filters are misconfigured
/// or if there are files that should be LFS but aren't tracked properly.
fn warn_if_modified_lfs(repo_root: &Path, strict: bool) -> Result<()> {
    match git::has_lfs(repo_root) {
        Ok(true) => {}
        Ok(false) => return Ok(()),
        Err(err) => {
            log::warn!("Git LFS detection failed: {:#}", err);
            return Ok(());
        }
    }

    // Perform comprehensive LFS health check
    let health_report = match git::check_lfs_health(repo_root) {
        Ok(report) => report,
        Err(err) => {
            log::warn!("Git LFS health check failed: {:#}", err);
            return Ok(());
        }
    };

    if !health_report.lfs_initialized {
        return Ok(());
    }

    // Check filter configuration
    if let Some(ref filter_status) = health_report.filter_status {
        if !filter_status.is_healthy() {
            let issues = filter_status.issues();
            if strict {
                return Err(anyhow!(
                    "Git LFS filters misconfigured: {}. Run 'git lfs install' to fix.",
                    issues.join("; ")
                ));
            } else {
                log::error!(
                    "Git LFS filters misconfigured: {}. Run 'git lfs install' to fix. This may cause data loss if LFS files are committed as pointers!",
                    issues.join("; ")
                );
            }
        }
    }

    // Check LFS status for untracked files
    if let Some(ref status_summary) = health_report.status_summary {
        if !status_summary.is_clean() {
            let issues = status_summary.issue_descriptions();
            if strict {
                return Err(anyhow!("Git LFS issues detected: {}", issues.join("; ")));
            } else {
                for issue in issues {
                    log::warn!("LFS issue: {}", issue);
                }
            }
        }
    }

    // Check for pointer file issues
    if !health_report.pointer_issues.is_empty() {
        for issue in &health_report.pointer_issues {
            if strict {
                return Err(anyhow!("LFS pointer issue: {}", issue.description()));
            } else {
                log::warn!("LFS pointer issue: {}", issue.description());
            }
        }
    }

    // Original modified files check
    let status_paths = match git::status_paths(repo_root) {
        Ok(paths) => paths,
        Err(err) => {
            log::warn!("Unable to read git status for LFS warning: {:#}", err);
            return Ok(());
        }
    };

    if status_paths.is_empty() {
        return Ok(());
    }

    let lfs_files = match git::list_lfs_files(repo_root) {
        Ok(files) => files,
        Err(err) => {
            log::warn!("Unable to list LFS files: {:#}", err);
            return Ok(());
        }
    };

    if lfs_files.is_empty() {
        log::warn!(
            "Git LFS detected but no tracked files were listed; review LFS changes manually."
        );
        return Ok(());
    }

    let modified = git::filter_modified_lfs_files(&status_paths, &lfs_files);
    if !modified.is_empty() {
        log::warn!("Modified Git LFS files detected: {}", modified.join(", "));
    }

    Ok(())
}

fn push_if_ahead(repo_root: &Path) -> Result<()> {
    match git::is_ahead_of_upstream(repo_root) {
        Ok(ahead) => {
            if !ahead {
                return Ok(());
            }
        }
        Err(GitError::NoUpstream) | Err(GitError::NoUpstreamConfigured) => {
            log::warn!("skipping push (no upstream configured)");
            return Ok(());
        }
        Err(err) => {
            return Err(anyhow!("upstream check failed: {:#}", err));
        }
    }
    if let Err(err) = git::push_upstream(repo_root) {
        bail!("Git push failed: the repository has unpushed commits but the push operation failed. Push manually to sync with upstream. Error: {:#}", err);
    }
    Ok(())
}

pub(super) fn find_task_status(
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

pub(super) fn run_ci_gate(resolved: &crate::config::Resolved) -> Result<()> {
    let enabled = resolved.config.agent.ci_gate_enabled.unwrap_or(true);
    let command = resolved
        .config
        .agent
        .ci_gate_command
        .as_deref()
        .unwrap_or("make ci")
        .trim();

    if !enabled {
        log::info!("CI gate disabled; skipping configured command '{command}'.");
        return Ok(());
    }

    if command.is_empty() {
        bail!("CI gate command is empty but CI gate is enabled. Set agent.ci_gate_command or disable the gate with agent.ci_gate_enabled=false.");
    }

    logging::with_scope(&format!("CI gate ({command})"), || {
        let status = runutil::shell_command(command)
            .current_dir(&resolved.repo_root)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| {
                format!(
                    "run CI gate command '{}' in {}",
                    command,
                    resolved.repo_root.display()
                )
            })?;

        if status.success() {
            return Ok(());
        }

        bail!(
            "CI failed: '{}' exited with code {:?}. Fix the linting, type-checking, or test failures before proceeding.",
            command,
            status.code()
        )
    })
}

pub(super) fn ci_gate_command_label(resolved: &crate::config::Resolved) -> String {
    resolved
        .config
        .agent
        .ci_gate_command
        .as_deref()
        .unwrap_or("make ci")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        AgentConfig, Config, NotificationConfig, QueueConfig, QueueFile, Runner, Task,
        TaskPriority, TaskStatus,
    };
    use crate::queue;
    use crate::testsupport::git as git_test;
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_queue(repo_root: &Path, status: TaskStatus) -> Result<()> {
        let task = Task {
            id: "RQ-0001".to_string(),
            status,
            title: "Test task".to_string(),
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
            depends_on: vec![],
            custom_fields: std::collections::HashMap::new(),
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
                update_task_before_run: None,
                fail_on_prerun_update_error: None,
                runner_cli: None,
                instruction_files: None,
                repoprompt_plan_required: Some(false),
                repoprompt_tool_injection: Some(false),
                ci_gate_command: Some("make ci".to_string()),
                ci_gate_enabled: Some(false),
                git_revert_mode: Some(GitRevertMode::Disabled),
                git_commit_push_enabled: Some(true),
                phases: Some(2),
                notification: NotificationConfig {
                    enabled: Some(false),
                    ..NotificationConfig::default()
                },
            },
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.json")),
                done_file: Some(PathBuf::from(".ralph/done.json")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(4),
                size_warning_threshold_kb: Some(500),
                task_count_warning_threshold: Some(500),
                max_dependency_depth: Some(10),
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
    fn resume_continue_session_requires_session_id() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let resolved = crate::config::Resolved {
            config: Config::default(),
            repo_root: temp_dir.path().to_path_buf(),
            queue_path: temp_dir.path().join("queue.json"),
            done_path: temp_dir.path().join("done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        };

        let mut session = ContinueSession {
            runner: Runner::Codex,
            model: crate::contracts::Model::Gpt52Codex,
            reasoning_effort: None,
            session_id: None,
            output_handler: None,
            output_stream: crate::runner::OutputStream::Terminal,
            ci_failure_retry_count: 0,
        };

        let err = resume_continue_session(&resolved, &mut session, "hello")
            .expect_err("expected missing session id error");
        assert!(err.to_string().contains("no session id"));
        Ok(())
    }

    #[test]
    fn post_run_supervise_commits_and_cleans_when_enabled() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        git_test::commit_all(temp.path(), "init")?;
        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resolved = resolved_for_repo(temp.path());
        post_run_supervise(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            true,
            None,
            None,
            None,
            false,
        )?;

        let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
        anyhow::ensure!(status.trim().is_empty(), "expected clean repo");

        let done_file = queue::load_queue_or_default(&resolved.done_path)?;
        anyhow::ensure!(
            done_file.tasks.iter().any(|t| t.id == "RQ-0001"),
            "expected task in done archive"
        );

        Ok(())
    }

    #[test]
    fn post_run_supervise_skips_commit_when_disabled() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        git_test::commit_all(temp.path(), "init")?;
        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resolved = resolved_for_repo(temp.path());
        post_run_supervise(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            false,
            None,
            None,
            None,
            false,
        )?;

        let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
        anyhow::ensure!(!status.trim().is_empty(), "expected dirty repo");
        Ok(())
    }

    #[test]
    fn post_run_supervise_backfills_missing_completed_at() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Done)?;
        git_test::commit_all(temp.path(), "init")?;

        let resolved = resolved_for_repo(temp.path());
        post_run_supervise(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            false,
            None,
            None,
            None,
            false,
        )?;

        let done_file = queue::load_queue_or_default(&resolved.done_path)?;
        let task = done_file
            .tasks
            .iter()
            .find(|t| t.id == "RQ-0001")
            .expect("expected task in done archive");
        let completed_at = task
            .completed_at
            .as_deref()
            .expect("completed_at should be stamped");

        crate::timeutil::parse_rfc3339(completed_at)?;

        Ok(())
    }

    #[test]
    fn post_run_supervise_errors_on_push_failure_when_enabled() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        git_test::commit_all(temp.path(), "init")?;

        let remote = TempDir::new()?;
        git_test::git_run(remote.path(), &["init", "--bare"])?;
        let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        )?;
        git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
        let missing_remote = temp.path().join("missing-remote");
        git_test::git_run(
            temp.path(),
            &[
                "remote",
                "set-url",
                "origin",
                missing_remote.to_str().unwrap(),
            ],
        )?;

        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resolved = resolved_for_repo(temp.path());
        let err = post_run_supervise(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            true,
            None,
            None,
            None,
            false,
        )
        .expect_err("expected push failure");
        assert!(format!("{err:#}").contains("Git push failed"));
        Ok(())
    }

    #[test]
    fn post_run_supervise_skips_push_when_disabled() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        git_test::commit_all(temp.path(), "init")?;

        let remote = TempDir::new()?;
        git_test::git_run(remote.path(), &["init", "--bare"])?;
        let branch = git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        )?;
        git_test::git_run(temp.path(), &["push", "-u", "origin", &branch])?;
        let missing_remote = temp.path().join("missing-remote");
        git_test::git_run(
            temp.path(),
            &[
                "remote",
                "set-url",
                "origin",
                missing_remote.to_str().unwrap(),
            ],
        )?;

        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resolved = resolved_for_repo(temp.path());
        post_run_supervise(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            false,
            None,
            None,
            None,
            false,
        )?;
        Ok(())
    }
}
