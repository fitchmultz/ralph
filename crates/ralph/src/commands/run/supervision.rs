//! Post-run supervision helpers.
//!
//! Handles post-run CI gating, queue/done updates, and git push/commit logic.

use super::logging;
use crate::contracts::{GitRevertMode, QueueFile, TaskStatus};
use crate::gitutil::GitError;
use crate::{gitutil, outpututil, queue, runutil, timeutil};
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
    let output = crate::runner::resume_session(
        session.runner,
        &resolved.repo_root,
        bins,
        session.model.clone(),
        session.reasoning_effort,
        session_id,
        message,
        resolved.config.agent.claude_permission_mode,
        None,
        session.output_handler.clone(),
        session.output_stream,
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

pub(crate) fn post_run_supervise(
    resolved: &crate::config::Resolved,
    task_id: &str,
    git_revert_mode: GitRevertMode,
    git_commit_push_enabled: bool,
    revert_prompt: Option<runutil::RevertPromptHandler>,
) -> Result<()> {
    let label = format!("PostRunSupervise for {}", task_id.trim());
    logging::with_scope(&label, || {
        let status = gitutil::status_porcelain(&resolved.repo_root)?;
        let is_dirty = !status.trim().is_empty();

        let (mut queue_file, mut done_file) =
            maintain_and_validate_queues(resolved, QueueMaintenanceSaveMode::SaveBothIfAnyRepaired)
                .context("Initial queue maintenance failed")?;

        let (mut task_status, task_title, mut in_done) =
            require_task_status(&queue_file, &done_file, task_id)?;

        if is_dirty {
            warn_if_modified_lfs(&resolved.repo_root);
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

            queue::archive_done_tasks(
                &resolved.queue_path,
                &resolved.done_path,
                &resolved.id_prefix,
                resolved.id_width,
            )
            .context("Queue archiving failed")?;

            finalize_git_state(resolved, task_id, &task_title, git_commit_push_enabled)
                .context("Git finalization failed")?;
            return Ok(());
        }

        if task_status == TaskStatus::Done && in_done {
            if git_commit_push_enabled {
                push_if_ahead(&resolved.repo_root).context("Git push failed")?;
            } else {
                log::info!("Auto git commit/push disabled; skipping push.");
            }
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

        let report = queue::archive_done_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
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
        Ok(())
    })
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
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
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
        gitutil::commit_all(&resolved.repo_root, &commit_message)?;
        push_if_ahead(&resolved.repo_root)?;
        gitutil::require_clean_repo_ignoring_paths(
            &resolved.repo_root,
            false,
            gitutil::RALPH_RUN_CLEAN_ALLOWED_PATHS,
        )?;
    } else {
        log::info!("Auto git commit/push disabled; leaving repo dirty after queue updates.");
    }
    Ok(())
}

fn warn_if_modified_lfs(repo_root: &Path) {
    match gitutil::has_lfs(repo_root) {
        Ok(true) => {}
        Ok(false) => return,
        Err(err) => {
            log::warn!("Git LFS detection failed: {:#}", err);
            return;
        }
    }

    let status_paths = match gitutil::status_paths(repo_root) {
        Ok(paths) => paths,
        Err(err) => {
            log::warn!("Unable to read git status for LFS warning: {:#}", err);
            return;
        }
    };

    if status_paths.is_empty() {
        return;
    }

    let lfs_files = match gitutil::list_lfs_files(repo_root) {
        Ok(files) => files,
        Err(err) => {
            log::warn!("Unable to list LFS files: {:#}", err);
            return;
        }
    };

    if lfs_files.is_empty() {
        log::warn!(
            "Git LFS detected but no tracked files were listed; review LFS changes manually."
        );
        return;
    }

    let modified = gitutil::filter_modified_lfs_files(&status_paths, &lfs_files);
    if modified.is_empty() {
        return;
    }

    log::warn!("Modified Git LFS files detected: {}", modified.join(", "));
}

fn push_if_ahead(repo_root: &Path) -> Result<()> {
    match gitutil::is_ahead_of_upstream(repo_root) {
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
    if let Err(err) = gitutil::push_upstream(repo_root) {
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
        AgentConfig, Config, QueueConfig, QueueFile, Runner, Task, TaskPriority, TaskStatus,
    };
    use crate::queue;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::Command;
    use tempfile::TempDir;

    fn git_run(repo_root: &Path, args: &[&str]) -> Result<()> {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()?;
        anyhow::ensure!(status.success(), "git {:?} failed", args);
        Ok(())
    }

    fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()?;
        anyhow::ensure!(output.status.success(), "git {:?} failed", args);
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn init_repo(dir: &Path) -> Result<()> {
        git_run(dir, &["init"])?;
        git_run(dir, &["config", "user.email", "test@example.com"])?;
        git_run(dir, &["config", "user.name", "Test User"])?;
        std::fs::create_dir_all(dir.join(".ralph"))?;
        Ok(())
    }

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

    fn commit_all(repo_root: &Path, message: &str) -> Result<()> {
        git_run(repo_root, &["add", "-A"])?;
        git_run(repo_root, &["commit", "-m", message])?;
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
                claude_permission_mode: Some(
                    crate::contracts::ClaudePermissionMode::BypassPermissions,
                ),
                repoprompt_plan_required: Some(false),
                repoprompt_tool_injection: Some(false),
                ci_gate_command: Some("make ci".to_string()),
                ci_gate_enabled: Some(false),
                git_revert_mode: Some(GitRevertMode::Disabled),
                git_commit_push_enabled: Some(true),
                phases: Some(2),
                update_task_before_run: Some(false),
            },
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.json")),
                done_file: Some(PathBuf::from(".ralph/done.json")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(4),
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
        init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        commit_all(temp.path(), "init")?;
        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resolved = resolved_for_repo(temp.path());
        post_run_supervise(&resolved, "RQ-0001", GitRevertMode::Disabled, true, None)?;

        let status = git_output(temp.path(), &["status", "--porcelain"])?;
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
        init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        commit_all(temp.path(), "init")?;
        std::fs::write(temp.path().join("work.txt"), "change")?;

        let resolved = resolved_for_repo(temp.path());
        post_run_supervise(&resolved, "RQ-0001", GitRevertMode::Disabled, false, None)?;

        let status = git_output(temp.path(), &["status", "--porcelain"])?;
        anyhow::ensure!(!status.trim().is_empty(), "expected dirty repo");
        Ok(())
    }

    #[test]
    fn post_run_supervise_backfills_missing_completed_at() -> Result<()> {
        let temp = TempDir::new()?;
        init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Done)?;
        commit_all(temp.path(), "init")?;

        let resolved = resolved_for_repo(temp.path());
        post_run_supervise(&resolved, "RQ-0001", GitRevertMode::Disabled, false, None)?;

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

        use time::format_description::well_known::Rfc3339;
        use time::OffsetDateTime;
        OffsetDateTime::parse(completed_at, &Rfc3339)?;

        Ok(())
    }

    #[test]
    fn post_run_supervise_errors_on_push_failure_when_enabled() -> Result<()> {
        let temp = TempDir::new()?;
        init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        commit_all(temp.path(), "init")?;

        let remote = TempDir::new()?;
        git_run(remote.path(), &["init", "--bare"])?;
        let branch = git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_run(
            temp.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        )?;
        git_run(temp.path(), &["push", "-u", "origin", &branch])?;
        let missing_remote = temp.path().join("missing-remote");
        git_run(
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
        let err = post_run_supervise(&resolved, "RQ-0001", GitRevertMode::Disabled, true, None)
            .expect_err("expected push failure");
        assert!(format!("{err:#}").contains("Git push failed"));
        Ok(())
    }

    #[test]
    fn post_run_supervise_skips_push_when_disabled() -> Result<()> {
        let temp = TempDir::new()?;
        init_repo(temp.path())?;
        write_queue(temp.path(), TaskStatus::Todo)?;
        commit_all(temp.path(), "init")?;

        let remote = TempDir::new()?;
        git_run(remote.path(), &["init", "--bare"])?;
        let branch = git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        git_run(
            temp.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        )?;
        git_run(temp.path(), &["push", "-u", "origin", &branch])?;
        let missing_remote = temp.path().join("missing-remote");
        git_run(
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
        post_run_supervise(&resolved, "RQ-0001", GitRevertMode::Disabled, false, None)?;
        Ok(())
    }
}
