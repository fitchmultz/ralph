use crate::config;
use crate::contracts::{Model, ProjectType, QueueFile, ReasoningEffort, Runner, TaskStatus};
use crate::gitutil::GitError;
use crate::{gitutil, outpututil, prompts, queue, runner, runutil, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct AgentOverrides {
    pub runner: Option<Runner>,
    pub model: Option<Model>,
    pub reasoning_effort: Option<ReasoningEffort>,
}

pub enum RunOutcome {
    NoTodo,
    Ran { task_id: String },
}

pub struct RunLoopOptions {
    /// 0 means "no limit"
    pub max_tasks: u32,
    pub agent_overrides: AgentOverrides,
    pub force: bool,
}

pub fn run_loop(resolved: &config::Resolved, opts: RunLoopOptions) -> Result<()> {
    let mut completed = 0u32;

    let (queue_file, repaired) = queue::load_queue_with_repair(
        &resolved.queue_path,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    queue::warn_if_repaired(&resolved.queue_path, repaired);

    let initial_todo_count = queue_file
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Todo)
        .count() as u32;

    if initial_todo_count == 0 {
        log::info!("No todo tasks found.");
        return Ok(());
    }

    log::info!("Starting run loop with {initial_todo_count} todo tasks.");

    loop {
        if opts.max_tasks != 0 && completed >= opts.max_tasks {
            log::info!("Reached max task limit ({completed}).");
            return Ok(());
        }

        match run_one(resolved, &opts.agent_overrides, opts.force)? {
            RunOutcome::NoTodo => {
                log::info!("No more todo tasks remaining.");
                return Ok(());
            }
            RunOutcome::Ran { task_id } => {
                completed += 1;
                log::info!("Completed {task_id} ({completed}/{initial_todo_count}).");
            }
        }
    }
}

pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
) -> Result<RunOutcome> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "run one", force)?;
    let (queue_file, repaired_queue) = queue::load_queue_with_repair(
        &resolved.queue_path,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    queue::warn_if_repaired(&resolved.queue_path, repaired_queue);
    let (done, repaired_done) = queue::load_queue_or_default_with_repair(
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    queue::warn_if_repaired(&resolved.done_path, repaired_done);
    let done_ref = if done.tasks.is_empty() && !resolved.done_path.exists() {
        None
    } else {
        Some(&done)
    };
    queue::validate_queue_set(
        &queue_file,
        done_ref,
        &resolved.id_prefix,
        resolved.id_width,
    )?;

    let idx = match queue_file
        .tasks
        .iter()
        .position(|t| t.status == TaskStatus::Todo)
    {
        Some(idx) => idx,
        None => {
            log::info!("No todo tasks found.");
            return Ok(RunOutcome::NoTodo);
        }
    };

    let task = queue_file.tasks[idx].clone();
    let task_id = task.id.trim().to_string();
    if task_id.is_empty() {
        bail!("Invalid task: selected task has an empty ID. Ensure the task has a valid ID (e.g., 'RQ-0001') in .ralph/queue.yaml.");
    }

    // Require a clean repo before we invoke the runner.
    // This prevents accidental destruction of unrelated user work on failure recovery.
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        &[".ralph/queue.yaml", ".ralph/done.yaml"],
    )?;

    let settings = resolve_run_agent_settings(resolved, &task, agent_overrides)?;

    log::info!(
        "Executing {task_id}: {title} (runner: {runner:?}, model: {model})",
        title = task.title,
        runner = settings.runner,
        model = settings.model.as_str()
    );
    let start = std::time::Instant::now();

    let bins = runner::resolve_binaries(&resolved.config.agent);

    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let prompt = prompts::render_worker_prompt(&template, project_type)?;
    // Two-pass mode enabled for implementation workflows
    let two_pass_plan = resolved.config.agent.two_pass_plan.unwrap_or(true);
    // Use configured permission mode (default: BypassPermissions)
    let permission_mode = resolved.config.agent.claude_permission_mode;

    let _output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: settings.runner,
            bins,
            model: settings.model,
            reasoning_effort: settings.reasoning_effort,
            prompt: &prompt,
            timeout: None,
            two_pass_plan,
            permission_mode,
        },
        runutil::RunnerErrorMessages {
            log_label: "runner",
            interrupted_msg: "Runner interrupted: the execution was canceled by the user or system. Uncommitted changes were reverted to maintain a clean repo state.",
            timeout_msg: "Runner timed out: the execution exceeded the allowed time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Runner terminated: the agent was stopped by a signal. Uncommitted changes were reverted. Rerunning the task is recommended.",
            non_zero_msg: |code| {
                format!(
                    "Runner failed: the agent exited with a non-zero code ({code}). Uncommitted changes were reverted. Rerunning the task is recommended after investigating the cause."
                )
            },
            other_msg: |err| {
                format!(
                    "Runner invocation failed: the agent could not be started or encountered an error. Uncommitted changes were reverted. Rerunning the task is recommended. Error: {:#}",
                    err
                )
            },
        },
    )?;

    let duration = start.elapsed();
    log::info!("Runner completed successfully for {task_id} in {duration:?}.");

    post_run_supervise(resolved, &task_id)?;
    Ok(RunOutcome::Ran { task_id })
}

fn resolve_run_agent_settings(
    resolved: &config::Resolved,
    task: &crate::contracts::Task,
    overrides: &AgentOverrides,
) -> Result<runner::AgentSettings> {
    runner::resolve_agent_settings(
        overrides.runner,
        overrides.model.clone(),
        overrides.reasoning_effort,
        task.agent.as_ref(),
        &resolved.config.agent,
    )
}
fn post_run_supervise(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let status = gitutil::status_porcelain(&resolved.repo_root)?;
    let is_dirty = !status.trim().is_empty();

    let (mut queue_file, repaired_queue) = queue::load_queue_with_repair(
        &resolved.queue_path,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    queue::warn_if_repaired(&resolved.queue_path, repaired_queue);
    let (mut done_file, repaired_done) = queue::load_queue_or_default_with_repair(
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    queue::warn_if_repaired(&resolved.done_path, repaired_done);
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

    let (mut task_status, task_title, mut in_done) =
        find_task_status(&queue_file, &done_file, task_id)
            .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;

    if is_dirty {
        if let Err(err) = run_make_ci(&resolved.repo_root) {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("CI gate failed: 'make ci' did not pass after the task completed. Uncommitted changes were reverted. Fix the issues reported by CI and try again. Error: {:#}", err);
        }

        let (reloaded_queue, repaired_queue) = queue::load_queue_with_repair(
            &resolved.queue_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?;
        queue::warn_if_repaired(&resolved.queue_path, repaired_queue);
        queue_file = reloaded_queue;
        let (reloaded_done, repaired_done) = queue::load_queue_or_default_with_repair(
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?;
        queue::warn_if_repaired(&resolved.done_path, repaired_done);
        done_file = reloaded_done;
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

        let (status_after, _title_after, in_done_after) =
            find_task_status(&queue_file, &done_file, task_id)
                .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;
        task_status = status_after;
        in_done = in_done_after;

        if task_status != TaskStatus::Done {
            if in_done {
                gitutil::revert_uncommitted(&resolved.repo_root)?;
                bail!("Task inconsistency: task {task_id} is archived in done.yaml but its status is not 'done'. Review the task state in .ralph/done.yaml.");
            }
            let now = timeutil::now_utc_rfc3339()?;
            queue::set_status(&mut queue_file, task_id, TaskStatus::Done, &now, None)?;
            queue::save_queue(&resolved.queue_path, &queue_file)?;
        }

        queue::archive_done_tasks(
            &resolved.queue_path,
            &resolved.done_path,
            &resolved.id_prefix,
            resolved.id_width,
        )?;

        let commit_message = outpututil::format_task_commit_message(task_id, &task_title);
        gitutil::commit_all(&resolved.repo_root, &commit_message)?;
        push_if_ahead(&resolved.repo_root)?;
        gitutil::require_clean_repo_ignoring_paths(
            &resolved.repo_root,
            false,
            &[".ralph/queue.yaml", ".ralph/done.yaml"],
        )?;
        return Ok(());
    }

    if task_status == TaskStatus::Done && in_done {
        push_if_ahead(&resolved.repo_root)?;
        return Ok(());
    }

    let mut changed = false;
    if task_status != TaskStatus::Done {
        if in_done {
            bail!("Task inconsistency: task {task_id} is archived in done.yaml but its status is not 'done'. Review the task state in .ralph/done.yaml.");
        }
        let now = timeutil::now_utc_rfc3339()?;
        queue::set_status(&mut queue_file, task_id, TaskStatus::Done, &now, None)?;
        queue::save_queue(&resolved.queue_path, &queue_file)?;
        changed = true;
    }

    let report = queue::archive_done_tasks(
        &resolved.queue_path,
        &resolved.done_path,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    if !report.moved_ids.is_empty() {
        changed = true;
    }

    if !changed {
        return Ok(());
    }

    let commit_message = outpututil::format_task_commit_message(task_id, &task_title);
    gitutil::commit_all(&resolved.repo_root, &commit_message)?;
    push_if_ahead(&resolved.repo_root)?;
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        false,
        &[".ralph/queue.yaml", ".ralph/done.yaml"],
    )?;
    Ok(())
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

fn find_task_status(
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

fn run_make_ci(repo_root: &Path) -> Result<()> {
    let status = Command::new("make")
        .arg("ci")
        .current_dir(repo_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("run make ci in {}", repo_root.display()))?;

    if status.success() {
        return Ok(());
    }

    bail!("CI failed: 'make ci' exited with code {:?}. Fix the linting, type-checking, or test failures before proceeding.", status.code())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        AgentConfig, ClaudePermissionMode, Config, Model, QueueConfig, Task, TaskAgent, TaskStatus,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn resolved_with_agent_defaults(
        runner: Option<Runner>,
        model: Option<Model>,
        effort: Option<ReasoningEffort>,
    ) -> config::Resolved {
        let dir = TempDir::new().expect("temp dir");
        let repo_root = dir.path().to_path_buf();

        let cfg = Config {
            agent: AgentConfig {
                runner,
                model,
                reasoning_effort: effort,
                codex_bin: Some("codex".to_string()),
                opencode_bin: Some("opencode".to_string()),
                gemini_bin: Some("gemini".to_string()),
                claude_bin: Some("claude".to_string()),
                two_pass_plan: Some(true),
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
            },
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.yaml")),
                done_file: Some(PathBuf::from(".ralph/done.yaml")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(4),
            },
            ..Config::default()
        };

        config::Resolved {
            config: cfg,
            repo_root: repo_root.clone(),
            queue_path: repo_root.join(".ralph/queue.yaml"),
            done_path: repo_root.join(".ralph/done.yaml"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.yaml")),
        }
    }

    fn base_task() -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test task".to_string(),
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
        }
    }

    #[test]
    fn resolve_run_agent_settings_task_agent_overrides_config() -> Result<()> {
        let resolved = resolved_with_agent_defaults(
            Some(Runner::Codex),
            Some(Model::Gpt52Codex),
            Some(ReasoningEffort::Medium),
        );

        let mut task = base_task();
        task.agent = Some(TaskAgent {
            runner: Some(Runner::Opencode),
            model: Some(Model::Gpt52),
            reasoning_effort: Some(ReasoningEffort::High),
        });

        let overrides = AgentOverrides::default();
        let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
        assert_eq!(settings.runner, Runner::Opencode);
        assert_eq!(settings.model, Model::Gpt52);
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn resolve_run_agent_settings_cli_overrides_task_agent_and_config() -> Result<()> {
        let resolved = resolved_with_agent_defaults(
            Some(Runner::Opencode),
            Some(Model::Gpt52),
            Some(ReasoningEffort::Low),
        );

        let mut task = base_task();
        task.agent = Some(TaskAgent {
            runner: Some(Runner::Opencode),
            model: Some(Model::Gpt52),
            reasoning_effort: Some(ReasoningEffort::Low),
        });

        let overrides = AgentOverrides {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
        };

        let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
        assert_eq!(settings.runner, Runner::Codex);
        assert_eq!(settings.model, Model::Gpt52Codex);
        assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::High));
        Ok(())
    }

    #[test]
    fn resolve_run_agent_settings_defaults_to_glm47_for_opencode_runner() -> Result<()> {
        // Config defaults to Codex + Gpt52Codex
        let resolved = resolved_with_agent_defaults(
            Some(Runner::Codex),
            Some(Model::Gpt52Codex),
            Some(ReasoningEffort::Medium),
        );

        let task = base_task();

        // Override runner to Opencode, but not model.
        // Should default to Glm47 to avoid model mismatch.
        let overrides = AgentOverrides {
            runner: Some(Runner::Opencode),
            model: None,
            reasoning_effort: None,
        };

        let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
        assert_eq!(settings.runner, Runner::Opencode);
        assert_eq!(settings.model, Model::Glm47);
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn resolve_run_agent_settings_defaults_to_gemini_flash_for_gemini_runner() -> Result<()> {
        // Config defaults to Codex + Gpt52Codex
        let resolved = resolved_with_agent_defaults(
            Some(Runner::Codex),
            Some(Model::Gpt52Codex),
            Some(ReasoningEffort::Medium),
        );

        let task = base_task();

        let overrides = AgentOverrides {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        };

        let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
        assert_eq!(settings.runner, Runner::Gemini);
        assert_eq!(settings.model.as_str(), "gemini-3-flash-preview");
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn resolve_run_agent_settings_effort_defaults_to_medium_for_codex_when_unspecified(
    ) -> Result<()> {
        let resolved =
            resolved_with_agent_defaults(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

        let task = base_task();
        let overrides = AgentOverrides::default();

        let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
        assert_eq!(settings.runner, Runner::Codex);
        assert_eq!(settings.model, Model::Gpt52Codex);
        assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::Medium));
        Ok(())
    }

    #[test]
    fn resolve_run_agent_settings_effort_is_ignored_for_opencode() -> Result<()> {
        let resolved = resolved_with_agent_defaults(
            Some(Runner::Opencode),
            Some(Model::Gpt52),
            Some(ReasoningEffort::Low),
        );

        let task = base_task();
        let overrides = AgentOverrides {
            runner: Some(Runner::Opencode),
            model: Some(Model::Gpt52),
            reasoning_effort: Some(ReasoningEffort::High),
        };

        let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
        assert_eq!(settings.runner, Runner::Opencode);
        assert_eq!(settings.model, Model::Gpt52);
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }
}
