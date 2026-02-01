//! Post-run supervision orchestration.
//!
//! Responsibilities:
//! - Orchestrate post-run workflow: CI gate, queue updates, git operations, notifications.
//! - Manage ContinueSession for session resumption.
//! - Coordinate celebration triggers and productivity stats.
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
use crate::contracts::GitRevertMode;
use crate::git;
use crate::notification;
use crate::productivity;
use crate::queue;
use crate::runutil;
use anyhow::{Context, Result, anyhow};

mod ci;
mod git_ops;
mod notify;
mod queue_ops;

// Re-export items needed by run/mod.rs and other modules
pub(crate) use ci::{ci_gate_command_label, run_ci_gate};
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

/// Resume a continue session with a message.
pub(crate) fn resume_continue_session(
    resolved: &crate::config::Resolved,
    session: &mut ContinueSession,
    message: &str,
) -> Result<crate::runner::RunnerOutput> {
    let session_id = match session.session_id.as_deref() {
        Some(session_id) => session_id,
        None => {
            if session.runner == crate::contracts::Runner::Kimi {
                ""
            } else {
                anyhow::bail!("Catastrophic: no session id captured; cannot Continue.");
            }
        }
    };
    let bins = crate::runner::resolve_binaries(&resolved.config.agent);
    // Use the stored runner_cli and phase_type from the session to preserve
    // CLI overrides and ensure phase-correct behavior for phase-aware runners.
    let output = crate::runner::resume_session(
        session.runner,
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
    )?;
    if let Some(new_id) = output.session_id.as_ref() {
        session.session_id = Some(new_id.clone());
    }
    Ok(output)
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
    revert_prompt: Option<runutil::RevertPromptHandler>,
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

            finalize_git_state(resolved, task_id, &task_title, git_commit_push_enabled)
                .context("Git finalization failed")?;

            // Trigger completion notification on successful completion
            let notify_config =
                build_notification_config(resolved, notify_on_complete, notify_sound);
            notification::notify_task_complete(task_id, &task_title, &notify_config);

            return Ok(());
        }

        if task_status == crate::contracts::TaskStatus::Done && in_done {
            if git_commit_push_enabled {
                push_if_ahead(&resolved.repo_root).context("Git push failed")?;
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

        finalize_git_state(resolved, task_id, &task_title, git_commit_push_enabled)
            .context("Git finalization failed")?;

        // Trigger completion notification on successful completion
        let notify_config = build_notification_config(resolved, notify_on_complete, notify_sound);
        notification::notify_task_complete(task_id, &task_title, &notify_config);

        Ok(())
    })
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
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
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
}
