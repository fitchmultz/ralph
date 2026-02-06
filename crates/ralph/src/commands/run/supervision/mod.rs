//! Post-run supervision orchestration.
//!
//! Responsibilities:
//! - Orchestrate post-run workflow: CI gate, queue updates, git operations, notifications.
//! - Manage ContinueSession for session resumption.
//! - Coordinate celebration triggers and productivity stats.
//! - Provide parallel-worker supervision without mutating queue/done.
//!
//! Not handled here:
//! - Individual concern implementations (see queue_ops.rs, git_ops.rs, ci.rs, notify.rs).
//! - Runner process execution (handled by phases module).
//!
//! Invariants/assumptions:
//! - post_run_supervise is called after task execution completes.
//! - Queue files are valid and accessible.
//! - Git repo state reflects task changes when is_dirty is true.

use crate::celebrations;
use crate::completions;
use crate::contracts::GitRevertMode;

use crate::git;
use crate::notification;
use crate::productivity;
use crate::queue;
use crate::runutil;
use anyhow::{Context, Result, anyhow, bail};

mod ci;
mod git_ops;
mod notify;
mod queue_ops;

// Re-export items needed by run/mod.rs and other modules
pub(crate) use ci::{ci_gate_command_label, run_ci_gate, run_ci_gate_with_continue_session};
use git_ops::{finalize_git_state, push_if_ahead, warn_if_modified_lfs};
use notify::build_notification_config;
pub(crate) use queue_ops::find_task_status;
use queue_ops::{
    QueueMaintenanceSaveMode, ensure_task_done_clean_or_bail, ensure_task_done_dirty_or_revert,
    maintain_and_validate_queues, require_task_status,
};

use super::logging;

/// Session state for continuing an interrupted task.
#[derive(Clone)]
pub(crate) struct ContinueSession {
    pub runner: crate::contracts::Runner,
    pub model: crate::contracts::Model,
    pub reasoning_effort: Option<crate::contracts::ReasoningEffort>,
    /// The runner CLI settings resolved for the run that created this continue session.
    /// These must be preserved to avoid losing CLI overrides / task-specific settings.
    pub runner_cli: crate::runner::ResolvedRunnerCliOptions,
    /// The phase that created this continue session. Must be preserved so phase-aware
    /// runners (e.g., Cursor) behave correctly on Continue.
    pub phase_type: super::PhaseType,
    pub session_id: Option<String>,
    pub output_handler: Option<crate::runner::OutputHandler>,
    pub output_stream: crate::runner::OutputStream,
    /// Number of automatic "fix CI and rerun" retries already sent for the current CI gate loop.
    /// Used to auto-enforce CI compliance without prompting for the first N failures.
    pub ci_failure_retry_count: u8,
}

/// Context for resuming a runner session during a post-run CI gate failure.
pub(crate) struct CiContinueContext<'a> {
    pub continue_session: &'a mut ContinueSession,
    /// Callback invoked after each resume, receiving both the output and the elapsed duration.
    /// The duration represents the wall-clock time spent in that specific resume session.
    pub on_resume:
        &'a mut dyn FnMut(&crate::runner::RunnerOutput, std::time::Duration) -> Result<()>,
}

/// Policy for pushing git commits after a run completes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PushPolicy {
    /// Require an existing upstream; skip push if none is configured.
    RequireUpstream,
    /// Allow creating an upstream (e.g., `git push -u origin HEAD`) when missing.
    AllowCreateUpstream,
}

/// Resume a continue session with a message.
///
/// Returns the runner output along with the wall-clock duration of the session.
/// The duration is measured from the start of the function to when the runner
/// output is received.
pub(crate) fn resume_continue_session(
    resolved: &crate::config::Resolved,
    session: &mut ContinueSession,
    message: &str,
) -> Result<(crate::runner::RunnerOutput, std::time::Duration)> {
    let start = std::time::Instant::now();
    let session_id = session
        .session_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Catastrophic: no session id captured; cannot Continue."))?;
    let bins = crate::runner::resolve_binaries(&resolved.config.agent);
    // Use the stored runner_cli and phase_type from the session to preserve
    // CLI overrides and ensure phase-correct behavior for phase-aware runners.
    let output = crate::runner::resume_session(
        session.runner.clone(),
        &resolved.repo_root,
        bins,
        session.model.clone(),
        session.reasoning_effort,
        session.runner_cli,
        session_id,
        message,
        resolved.config.agent.claude_permission_mode,
        None,
        session.output_handler.clone(),
        session.output_stream,
        session.phase_type,
        None,
    )?;
    let elapsed = start.elapsed();
    if let Some(new_id) = output.session_id.as_ref() {
        session.session_id = Some(new_id.clone());
    }
    Ok((output, elapsed))
}

/// Main post-run supervision entry point.
///
/// Orchestrates the post-run workflow:
/// 1. Check git status (dirty vs clean)
/// 2. Run CI gate if dirty
/// 3. Update queue/done files
/// 4. Commit and push if enabled
/// 5. Trigger notifications and celebrations
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_run_supervise(
    resolved: &crate::config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    ci_continue: Option<CiContinueContext<'_>>,
    notify_on_complete: Option<bool>,
    notify_sound: Option<bool>,
    lfs_check: bool,
    no_progress: bool,
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
            let mut ci_continue = ci_continue;
            if let Some(ci_continue) = ci_continue.as_mut() {
                let continue_session = &mut *ci_continue.continue_session;
                let on_resume = &mut *ci_continue.on_resume;
                if continue_session
                    .session_id
                    .as_deref()
                    .unwrap_or("")
                    .is_empty()
                {
                    log::warn!(
                        "CI gate continue requested but no session id; falling back to standard CI gate handling."
                    );
                    if let Err(err) = run_ci_gate(resolved) {
                        let outcome = runutil::apply_git_revert_mode(
                            &resolved.repo_root,
                            git_revert_mode,
                            "CI gate failure",
                            revert_prompt.as_ref(),
                        )?;
                        anyhow::bail!(
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
                } else if let Err(err) = ci::run_ci_gate_with_continue_session(
                    resolved,
                    git_revert_mode,
                    revert_prompt.as_ref(),
                    continue_session,
                    |output, elapsed| on_resume(output, elapsed),
                ) {
                    let outcome = runutil::apply_git_revert_mode(
                        &resolved.repo_root,
                        git_revert_mode,
                        "CI gate failure",
                        revert_prompt.as_ref(),
                    )?;
                    anyhow::bail!(
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
            } else if let Err(err) = run_ci_gate(resolved) {
                let outcome = runutil::apply_git_revert_mode(
                    &resolved.repo_root,
                    git_revert_mode,
                    "CI gate failure",
                    revert_prompt.as_ref(),
                )?;
                anyhow::bail!(
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

            // Trigger celebration and record productivity stats BEFORE git commit
            // so productivity.json gets committed along with other changes
            trigger_celebration(resolved, task_id, &task_title, no_progress);

            finalize_git_state(
                resolved,
                task_id,
                &task_title,
                git_commit_push_enabled,
                push_policy,
            )
            .context("Git finalization failed")?;

            // Trigger completion notification on successful completion
            let notify_config =
                build_notification_config(resolved, notify_on_complete, notify_sound);
            notification::notify_task_complete(task_id, &task_title, &notify_config);

            return Ok(());
        }

        if task_status == crate::contracts::TaskStatus::Done && in_done {
            if git_commit_push_enabled {
                push_if_ahead(&resolved.repo_root, push_policy).context("Git push failed")?;
            } else {
                log::info!("Auto git commit/push disabled; skipping push.");
            }

            // Trigger completion notification on successful completion
            let notify_config =
                build_notification_config(resolved, notify_on_complete, notify_sound);
            notification::notify_task_complete(task_id, &task_title, &notify_config);

            // Trigger celebration and record productivity stats
            trigger_celebration(resolved, task_id, &task_title, no_progress);

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

        // Trigger celebration and record productivity stats BEFORE git commit
        // so productivity.json gets committed along with other changes
        trigger_celebration(resolved, task_id, &task_title, no_progress);

        finalize_git_state(
            resolved,
            task_id,
            &task_title,
            git_commit_push_enabled,
            push_policy,
        )
        .context("Git finalization failed")?;

        // Trigger completion notification on successful completion
        let notify_config = build_notification_config(resolved, notify_on_complete, notify_sound);
        notification::notify_task_complete(task_id, &task_title, &notify_config);

        Ok(())
    })
}

/// Post-run supervision for parallel workers.
///
/// Ensures completion signals are present, restores shared bookkeeping files,
/// and commits/pushes only the worker's task changes without mutating queue/done.
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_run_supervise_parallel_worker(
    resolved: &crate::config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    push_policy: PushPolicy,
    revert_prompt: Option<runutil::RevertPromptHandler>,
    ci_continue: Option<CiContinueContext<'_>>,
    lfs_check: bool,
) -> Result<()> {
    let label = format!("PostRunSuperviseParallelWorker for {}", task_id.trim());
    logging::with_scope(&label, || {
        let status = git::status_porcelain(&resolved.repo_root)?;
        let is_dirty = !status.trim().is_empty();

        if is_dirty {
            if let Err(err) = warn_if_modified_lfs(&resolved.repo_root, lfs_check) {
                return Err(anyhow!(
                    "LFS validation failed: {}. Use --lfs-check to enable strict validation or fix the LFS issues.",
                    err
                ));
            }
            let mut ci_continue = ci_continue;
            if let Some(ci_continue) = ci_continue.as_mut() {
                let continue_session = &mut *ci_continue.continue_session;
                let on_resume = &mut *ci_continue.on_resume;
                if continue_session
                    .session_id
                    .as_deref()
                    .unwrap_or("")
                    .is_empty()
                {
                    log::warn!(
                        "CI gate continue requested but no session id; falling back to standard CI gate handling."
                    );
                    if let Err(err) = run_ci_gate(resolved) {
                        let outcome = runutil::apply_git_revert_mode(
                            &resolved.repo_root,
                            git_revert_mode,
                            "CI gate failure",
                            revert_prompt.as_ref(),
                        )?;
                        anyhow::bail!(
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
                } else if let Err(err) = ci::run_ci_gate_with_continue_session(
                    resolved,
                    git_revert_mode,
                    revert_prompt.as_ref(),
                    continue_session,
                    |output, elapsed| on_resume(output, elapsed),
                ) {
                    let outcome = runutil::apply_git_revert_mode(
                        &resolved.repo_root,
                        git_revert_mode,
                        "CI gate failure",
                        revert_prompt.as_ref(),
                    )?;
                    anyhow::bail!(
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
            } else if let Err(err) = run_ci_gate(resolved) {
                let outcome = runutil::apply_git_revert_mode(
                    &resolved.repo_root,
                    git_revert_mode,
                    "CI gate failure",
                    revert_prompt.as_ref(),
                )?;
                anyhow::bail!(
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
        }

        restore_parallel_worker_bookkeeping(resolved)?;
        ensure_completion_signal(resolved, task_id)?;
        stage_completion_signal(resolved, task_id)?;

        let status = git::status_porcelain(&resolved.repo_root)?;
        if status.trim().is_empty() {
            return Ok(());
        }

        if git_commit_push_enabled {
            let task_title = task_title_from_queue_or_done(resolved, task_id)?.unwrap_or_default();
            finalize_git_state(
                resolved,
                task_id,
                &task_title,
                git_commit_push_enabled,
                push_policy,
            )
            .context("Git finalization failed")?;
        } else {
            log::info!("Auto git commit/push disabled; leaving repo dirty after worker run.");
        }

        Ok(())
    })
}

fn ensure_completion_signal(resolved: &crate::config::Resolved, task_id: &str) -> Result<()> {
    if completions::read_completion_signal(&resolved.repo_root, task_id)?.is_some() {
        return Ok(());
    }

    let signal_path = completions::completion_signal_path(&resolved.repo_root, task_id)?;
    bail!(
        "Completion signal for {} is missing at {}.\n\nRemediation options:\n  1. Re-run Phase 3 for the task to generate a completion signal (e.g., ralph run one --phases 3 --id {})\n  2. Manually finalize the task: ralph task done {} (or ralph task rejected {})\n\nNote: Parallel workers require an explicit completion signal; Ralph will not infer Done.",
        task_id,
        signal_path.display(),
        task_id,
        task_id,
        task_id
    )
}

fn stage_completion_signal(resolved: &crate::config::Resolved, task_id: &str) -> Result<()> {
    let signal_path = completions::completion_signal_path(&resolved.repo_root, task_id)?;
    if !signal_path.exists() {
        return Ok(());
    }
    git::add_paths_force(&resolved.repo_root, &[signal_path])
        .context("force-add completion signal")?;
    Ok(())
}

fn task_title_from_queue_or_done(
    resolved: &crate::config::Resolved,
    task_id: &str,
) -> Result<Option<String>> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    if let Some(task) = queue_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
    if let Some(task) = done_file.tasks.iter().find(|t| t.id.trim() == task_id) {
        return Ok(Some(task.title.clone()));
    }
    Ok(None)
}

fn restore_parallel_worker_bookkeeping(resolved: &crate::config::Resolved) -> Result<()> {
    let productivity_path = resolved
        .repo_root
        .join(".ralph")
        .join("cache")
        .join("productivity.json");
    let paths = vec![
        resolved.queue_path.clone(),
        resolved.done_path.clone(),
        productivity_path,
    ];
    git::restore_tracked_paths_to_head(&resolved.repo_root, &paths)
        .context("restore queue/done/productivity to HEAD")?;
    Ok(())
}

/// Trigger celebration and record productivity stats for task completion.
fn trigger_celebration(
    resolved: &crate::config::Resolved,
    task_id: &str,
    task_title: &str,
    no_progress: bool,
) {
    // Check if stats tracking is enabled (default: true)
    let stats_enabled = resolved.config.tui.stats_enabled.unwrap_or(true);

    if stats_enabled {
        // Record the completion in productivity stats
        let cache_dir = resolved.repo_root.join(".ralph").join("cache");
        match productivity::record_task_completion_by_id(task_id, task_title, &cache_dir) {
            Ok(result) => {
                // Check if celebrations are enabled and we're in a terminal
                if celebrations::should_celebrate(Some(&resolved.config), no_progress) {
                    let celebration =
                        celebrations::celebrate_task_completion(task_id, task_title, &result);
                    println!("{}", celebration);
                }

                // Mark milestone as celebrated if one was achieved
                if let Some(threshold) = result.milestone_achieved
                    && let Err(err) = productivity::mark_milestone_celebrated(&cache_dir, threshold)
                {
                    log::debug!("Failed to mark milestone as celebrated: {}", err);
                }
            }
            Err(err) => {
                log::debug!("Failed to record productivity stats: {}", err);
            }
        }
    } else if celebrations::should_celebrate(Some(&resolved.config), no_progress) {
        // Stats disabled but celebrations still enabled - show simple celebration
        let celebration = celebrations::celebrate_standard(task_id, task_title);
        println!("{}", celebration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completions;
    use crate::constants::limits::CI_GATE_AUTO_RETRY_LIMIT;
    use crate::contracts::{
        AgentConfig, Config, NotificationConfig, QueueConfig, QueueFile, Runner, Task,
        TaskPriority, TaskStatus,
    };
    use crate::queue;
    use crate::testsupport::git as git_test;
    use crate::testsupport::runner::create_fake_runner;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Arc;
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
                update_task_before_run: None,
                fail_on_prerun_update_error: None,
                runner_cli: None,
                phase_overrides: None,
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
                webhook: crate::contracts::WebhookConfig::default(),
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
            },
            tui: crate::contracts::TuiConfig {
                auto_archive_terminal: None,
                celebrations_enabled: Some(false),
                stats_enabled: Some(false),
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
            runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
            phase_type: crate::commands::run::PhaseType::Implementation,
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
            PushPolicy::RequireUpstream,
            None,
            None,
            None,
            None,
            false,
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
            PushPolicy::RequireUpstream,
            None,
            None,
            None,
            None,
            false,
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
            PushPolicy::RequireUpstream,
            None,
            None,
            None,
            None,
            false,
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
            PushPolicy::RequireUpstream,
            None,
            None,
            None,
            None,
            false,
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
            PushPolicy::RequireUpstream,
            None,
            None,
            None,
            None,
            false,
            false,
        )?;
        Ok(())
    }

    #[test]
    fn post_run_supervise_allows_productivity_json_dirty() -> Result<()> {
        // Regression test: ensure productivity.json doesn't block task completion
        // See: supervisor triggering revert prompt due to productivity.json being dirty
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Done)?;
        git_test::commit_all(temp.path(), "init")?;

        // Create the cache directory and productivity.json file (simulating what
        // trigger_celebration does when recording stats)
        let cache_dir = temp.path().join(".ralph").join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        std::fs::write(
            cache_dir.join("productivity.json"),
            r#"{"version":1,"total_completed":1}"#,
        )?;

        // Also create a real work file that should be committed
        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resolved = resolved_for_repo(temp.path());
        // This should succeed even though productivity.json is untracked
        post_run_supervise(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            true, // git_commit_push_enabled = true
            PushPolicy::RequireUpstream,
            None,
            None,
            None,
            None,
            false,
            false,
        )?;

        // Verify the task is in done
        let done_file = queue::load_queue_or_default(&resolved.done_path)?;
        anyhow::ensure!(
            done_file.tasks.iter().any(|t| t.id == "RQ-0001"),
            "expected task in done archive"
        );

        // Verify the repo is clean (productivity.json was committed along with other changes)
        let status = git_test::git_output(temp.path(), &["status", "--porcelain"])?;
        anyhow::ensure!(
            status.trim().is_empty(),
            "expected clean repo after commit, but found: {}",
            status
        );

        Ok(())
    }

    #[test]
    fn post_run_supervise_ci_gate_continue_resumes_session() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        git_test::commit_all(temp.path(), "init")?;

        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resume_args = temp.path().join("resume-args.txt");
        let runner_script = format!(
            r#"#!/bin/sh
set -e
echo "$@" > "{resume_args}"
echo '{{"type":"text","part":{{"text":"resume"}}}}'
echo '{{"sessionID":"sess-123"}}'
"#,
            resume_args = resume_args.display()
        );
        let runner_path = create_fake_runner(temp.path(), "opencode", &runner_script)?;

        let ci_pass = temp.path().join("ci-pass.txt");
        let ci_command = format!("test -f {}", ci_pass.display());

        let mut resolved = resolved_for_repo(temp.path());
        resolved.config.agent.ci_gate_enabled = Some(true);
        resolved.config.agent.ci_gate_command = Some(ci_command);
        resolved.config.agent.opencode_bin = Some(runner_path.to_str().unwrap().to_string());

        let prompt_handler: runutil::RevertPromptHandler =
            Arc::new(|_context| runutil::RevertDecision::Continue {
                message: "fix the ci gate".to_string(),
            });

        let mut continue_session = ContinueSession {
            runner: Runner::Opencode,
            model: crate::contracts::Model::Custom("test-model".to_string()),
            reasoning_effort: None,
            runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
            phase_type: crate::commands::run::PhaseType::Review,
            session_id: Some("sess-123".to_string()),
            output_handler: None,
            output_stream: crate::runner::OutputStream::Terminal,
            ci_failure_retry_count: CI_GATE_AUTO_RETRY_LIMIT,
        };

        let mut on_resume =
            |_output: &crate::runner::RunnerOutput, _elapsed: std::time::Duration| -> Result<()> {
                std::fs::write(&ci_pass, "ok")?;
                Ok(())
            };

        post_run_supervise(
            &resolved,
            "RQ-0001",
            GitRevertMode::Ask,
            false,
            PushPolicy::RequireUpstream,
            Some(prompt_handler),
            Some(CiContinueContext {
                continue_session: &mut continue_session,
                on_resume: &mut on_resume,
            }),
            None,
            None,
            false,
            false,
        )?;

        let args = std::fs::read_to_string(&resume_args)?;
        anyhow::ensure!(
            args.contains("fix the ci gate"),
            "expected resume args to include continue message, got: {}",
            args
        );

        Ok(())
    }

    #[test]
    fn continue_session_preserves_runner_cli_options() {
        // Verify that ContinueSession correctly stores and preserves runner_cli options.
        // This is a regression test for the bug where runner_cli was re-resolved from
        // config on Continue, losing CLI overrides.
        use crate::contracts::{
            RunnerApprovalMode, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
            RunnerVerbosity, UnsupportedOptionPolicy,
        };

        let custom_runner_cli = crate::runner::ResolvedRunnerCliOptions {
            output_format: RunnerOutputFormat::StreamJson,
            verbosity: RunnerVerbosity::Quiet,
            approval_mode: RunnerApprovalMode::Safe,
            sandbox: RunnerSandboxMode::Enabled,
            plan_mode: RunnerPlanMode::Enabled,
            unsupported_option_policy: UnsupportedOptionPolicy::Error,
        };

        let session = ContinueSession {
            runner: Runner::Codex,
            model: crate::contracts::Model::Gpt52Codex,
            reasoning_effort: None,
            runner_cli: custom_runner_cli,
            phase_type: crate::commands::run::PhaseType::Implementation,
            session_id: Some("test-session".to_string()),
            output_handler: None,
            output_stream: crate::runner::OutputStream::Terminal,
            ci_failure_retry_count: 0,
        };

        // Verify the stored runner_cli matches what was set
        assert_eq!(session.runner_cli.verbosity, RunnerVerbosity::Quiet);
        assert_eq!(session.runner_cli.approval_mode, RunnerApprovalMode::Safe);
        assert_eq!(session.runner_cli.sandbox, RunnerSandboxMode::Enabled);
        assert_eq!(session.runner_cli.plan_mode, RunnerPlanMode::Enabled);
        assert_eq!(
            session.runner_cli.unsupported_option_policy,
            UnsupportedOptionPolicy::Error
        );
    }

    #[test]
    fn continue_session_preserves_phase_type() {
        // Verify that ContinueSession correctly stores and preserves the phase type.
        // This is a regression test for the bug where PhaseType::Implementation was
        // hardcoded for all continues, breaking phase-aware runners.
        use crate::commands::run::PhaseType;

        // Test Planning phase
        let planning_session = ContinueSession {
            runner: Runner::Codex,
            model: crate::contracts::Model::Gpt52Codex,
            reasoning_effort: None,
            runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
            phase_type: PhaseType::Planning,
            session_id: Some("test-session".to_string()),
            output_handler: None,
            output_stream: crate::runner::OutputStream::Terminal,
            ci_failure_retry_count: 0,
        };
        assert_eq!(planning_session.phase_type, PhaseType::Planning);

        // Test Implementation phase
        let impl_session = ContinueSession {
            runner: Runner::Codex,
            model: crate::contracts::Model::Gpt52Codex,
            reasoning_effort: None,
            runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
            phase_type: PhaseType::Implementation,
            session_id: Some("test-session".to_string()),
            output_handler: None,
            output_stream: crate::runner::OutputStream::Terminal,
            ci_failure_retry_count: 0,
        };
        assert_eq!(impl_session.phase_type, PhaseType::Implementation);

        // Test Review phase
        let review_session = ContinueSession {
            runner: Runner::Codex,
            model: crate::contracts::Model::Gpt52Codex,
            reasoning_effort: None,
            runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
            phase_type: PhaseType::Review,
            session_id: Some("test-session".to_string()),
            output_handler: None,
            output_stream: crate::runner::OutputStream::Terminal,
            ci_failure_retry_count: 0,
        };
        assert_eq!(review_session.phase_type, PhaseType::Review);

        // Test SinglePhase
        let single_session = ContinueSession {
            runner: Runner::Codex,
            model: crate::contracts::Model::Gpt52Codex,
            reasoning_effort: None,
            runner_cli: crate::runner::ResolvedRunnerCliOptions::default(),
            phase_type: PhaseType::SinglePhase,
            session_id: Some("test-session".to_string()),
            output_handler: None,
            output_stream: crate::runner::OutputStream::Terminal,
            ci_failure_retry_count: 0,
        };
        assert_eq!(single_session.phase_type, PhaseType::SinglePhase);
    }

    #[test]
    fn post_run_parallel_worker_restores_bookkeeping_and_requires_signal() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_root = temp_dir.path();
        git_test::init_repo(repo_root)?;

        let cache_dir = repo_root.join(".ralph/cache");
        std::fs::create_dir_all(&cache_dir)?;

        write_queue(repo_root, TaskStatus::Todo)?;
        queue::save_queue(
            &repo_root.join(".ralph/done.json"),
            &QueueFile {
                version: 1,
                tasks: vec![],
            },
        )?;
        let productivity_path = cache_dir.join("productivity.json");
        std::fs::write(&productivity_path, "{\"stats\":[]}")?;
        git_test::commit_all(repo_root, "init queue/done/productivity")?;

        // Pre-create the completion signal (parallel workers now require explicit signals)
        let signal = completions::CompletionSignal {
            task_id: "RQ-0001".to_string(),
            status: TaskStatus::Done,
            notes: vec![],
            runner_used: None,
            model_used: None,
        };
        completions::write_completion_signal(repo_root, &signal)?;

        let resolved = resolved_for_repo(repo_root);
        let queue_before = std::fs::read_to_string(&resolved.queue_path)?;
        let done_before = std::fs::read_to_string(&resolved.done_path)?;
        let productivity_before = std::fs::read_to_string(&productivity_path)?;

        // Dirty the bookkeeping files
        std::fs::write(&resolved.queue_path, "{\"version\":1,\"tasks\":[]}")?;
        std::fs::write(&resolved.done_path, "{\"version\":1,\"tasks\":[]}")?;
        std::fs::write(&productivity_path, "{\"stats\":[\"changed\"]}")?;

        post_run_supervise_parallel_worker(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            false,
            PushPolicy::RequireUpstream,
            None,
            None,
            false,
        )?;

        assert_eq!(std::fs::read_to_string(&resolved.queue_path)?, queue_before);
        assert_eq!(std::fs::read_to_string(&resolved.done_path)?, done_before);
        assert_eq!(
            std::fs::read_to_string(&productivity_path)?,
            productivity_before
        );

        let signal_path = completions::completion_signal_path(repo_root, "RQ-0001")?;
        assert!(signal_path.exists(), "completion signal should exist");

        let status_paths = git::status_paths(repo_root)?;
        let queue_rel = resolved
            .queue_path
            .strip_prefix(repo_root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let done_rel = resolved
            .done_path
            .strip_prefix(repo_root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let productivity_rel = productivity_path
            .strip_prefix(repo_root)
            .unwrap()
            .to_string_lossy()
            .to_string();

        assert!(
            !status_paths.contains(&queue_rel),
            "queue.json should be restored to HEAD"
        );
        assert!(
            !status_paths.contains(&done_rel),
            "done.json should be restored to HEAD"
        );
        assert!(
            !status_paths.contains(&productivity_rel),
            "productivity.json should be restored to HEAD"
        );

        Ok(())
    }

    #[test]
    fn post_run_parallel_worker_force_adds_signal_when_ignored() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_root = temp_dir.path();
        git_test::init_repo(repo_root)?;

        let gitignore_path = repo_root.join(".gitignore");
        std::fs::write(&gitignore_path, ".ralph/cache/completions\n")?;
        git_test::git_run(repo_root, &["add", ".gitignore"])?;
        git_test::commit_all(repo_root, "ignore completions")?;

        write_queue(repo_root, TaskStatus::Todo)?;
        queue::save_queue(
            &repo_root.join(".ralph/done.json"),
            &QueueFile {
                version: 1,
                tasks: vec![],
            },
        )?;
        git_test::commit_all(repo_root, "init queue/done")?;

        // Pre-create the completion signal (parallel workers now require explicit signals)
        let signal = completions::CompletionSignal {
            task_id: "RQ-0001".to_string(),
            status: TaskStatus::Done,
            notes: vec![],
            runner_used: None,
            model_used: None,
        };
        completions::write_completion_signal(repo_root, &signal)?;

        let resolved = resolved_for_repo(repo_root);
        post_run_supervise_parallel_worker(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            false,
            PushPolicy::RequireUpstream,
            None,
            None,
            false,
        )?;

        let signal_path = completions::completion_signal_path(repo_root, "RQ-0001")?;
        assert!(signal_path.exists(), "completion signal should exist");

        let signal_rel = signal_path
            .strip_prefix(repo_root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let status_paths = git::status_paths(repo_root)?;
        assert!(
            status_paths.contains(&signal_rel),
            "completion signal should be staged even if ignored"
        );

        Ok(())
    }

    #[test]
    fn post_run_parallel_worker_errors_when_completion_signal_missing_and_does_not_create()
    -> Result<()> {
        let temp_dir = TempDir::new()?;
        let repo_root = temp_dir.path();
        git_test::init_repo(repo_root)?;

        let cache_dir = repo_root.join(".ralph/cache");
        std::fs::create_dir_all(&cache_dir)?;

        write_queue(repo_root, TaskStatus::Todo)?;
        queue::save_queue(
            &repo_root.join(".ralph/done.json"),
            &QueueFile {
                version: 1,
                tasks: vec![],
            },
        )?;

        let productivity_path = cache_dir.join("productivity.json");
        std::fs::write(&productivity_path, "{\"stats\":[]}")?;
        git_test::commit_all(repo_root, "init queue/done/productivity")?;

        // Dirty bookkeeping files to ensure we still restore them even on error.
        let resolved = resolved_for_repo(repo_root);
        let queue_before = std::fs::read_to_string(&resolved.queue_path)?;
        let done_before = std::fs::read_to_string(&resolved.done_path)?;
        let productivity_before = std::fs::read_to_string(&productivity_path)?;

        std::fs::write(&resolved.queue_path, "{\"version\":1,\"tasks\":[]}")?;
        std::fs::write(&resolved.done_path, "{\"version\":1,\"tasks\":[]}")?;
        std::fs::write(&productivity_path, "{\"stats\":[\"changed\"]}")?;

        // Intentionally do NOT create a completion signal.
        let err = post_run_supervise_parallel_worker(
            &resolved,
            "RQ-0001",
            GitRevertMode::Disabled,
            false,
            PushPolicy::RequireUpstream,
            None,
            None,
            false,
        )
        .expect_err("expected missing completion signal error");

        let msg = err.to_string();
        assert!(msg.contains("Completion signal"), "unexpected error: {msg}");
        assert!(
            msg.contains("Remediation options"),
            "unexpected error: {msg}"
        );
        assert!(msg.contains("ralph task done"), "unexpected error: {msg}");

        // Ensure we did NOT create a default signal.
        let signal_path = completions::completion_signal_path(repo_root, "RQ-0001")?;
        assert!(
            !signal_path.exists(),
            "completion signal must not be created implicitly"
        );

        // Ensure bookkeeping restoration still happened.
        assert_eq!(std::fs::read_to_string(&resolved.queue_path)?, queue_before);
        assert_eq!(std::fs::read_to_string(&resolved.done_path)?, done_before);
        assert_eq!(
            std::fs::read_to_string(&productivity_path)?,
            productivity_before
        );

        Ok(())
    }
}
