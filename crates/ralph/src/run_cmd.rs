//! Run command orchestration and supervision.
//!
//! This module owns task selection, queue bookkeeping, and post-run supervision.
//! Phase-specific prompt/runner execution lives in `run_cmd::phases`.

use crate::config;
use crate::contracts::{ProjectType, QueueFile, TaskStatus};
use crate::gitutil::GitError;
use crate::promptflow;
use crate::{gitutil, outpututil, prompts, queue, runner, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

mod phases;

// Preserve existing `run_cmd.rs` unit tests which call `apply_phase3_completion_signal` directly.
#[allow(unused_imports)]
pub(crate) use phases::apply_phase3_completion_signal;

pub use crate::agent::AgentOverrides;

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

    let queue_file = queue::load_queue(&resolved.queue_path)?;

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

pub fn run_one_with_id(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    task_id: &str,
    output_handler: Option<runner::OutputHandler>,
) -> Result<()> {
    // Re-use run_one logic but target specific ID.
    // However, run_one finds the task based on status logic (Todo vs Doing).
    // run_one_with_id implies we selected a specific task.
    // We should probably adapt run_one_logic to take an optional task_id.
    // For now, let's just delegate to a shared implementation.
    run_one_impl(
        resolved,
        agent_overrides,
        force,
        Some(task_id),
        output_handler,
    )
    .map(|_| ())
}

pub fn run_one(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
) -> Result<RunOutcome> {
    run_one_impl(resolved, agent_overrides, force, None, None)
}

fn run_one_impl(
    resolved: &config::Resolved,
    agent_overrides: &AgentOverrides,
    force: bool,
    target_task_id: Option<&str>,
    output_handler: Option<runner::OutputHandler>,
) -> Result<RunOutcome> {
    let _queue_lock = queue::acquire_queue_lock(&resolved.repo_root, "run one", force)?;
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
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

    // Determine execution shape and policy
    let phases: u8 = agent_overrides
        .phases
        .or(resolved.config.agent.phases)
        .unwrap_or(2);
    if !(1..=3).contains(&phases) {
        bail!("Invalid phases value: {} (expected 1, 2, or 3)", phases);
    }

    let rp_required = agent_overrides
        .repoprompt_required
        .or(resolved.config.agent.require_repoprompt)
        .unwrap_or(false);

    let policy = promptflow::PromptPolicy {
        require_repoprompt: rp_required,
    };

    // --- Task Selection ---
    // Prefer resuming a `doing` task (crash recovery), otherwise take first runnable `todo`.
    let task_idx = if let Some(id) = target_task_id {
        let needle = id.trim();
        if needle.is_empty() {
            bail!("Target task id is empty");
        }
        let idx = queue_file
            .tasks
            .iter()
            .position(|t| t.id.trim() == needle)
            .ok_or_else(|| anyhow!("Target task {} not found in queue", needle))?;
        let status = queue_file.tasks[idx].status;
        if status == TaskStatus::Done || status == TaskStatus::Rejected {
            bail!(
                "Target task {} is not runnable (status: {}). Choose a todo/doing task.",
                needle,
                status
            );
        }
        idx
    } else if let Some(idx) = queue_file
        .tasks
        .iter()
        .position(|t| t.status == TaskStatus::Doing)
    {
        idx
    } else {
        match queue_file.tasks.iter().position(|t| {
            t.status == TaskStatus::Todo && queue::are_dependencies_met(t, &queue_file, done_ref)
        }) {
            Some(idx) => idx,
            None => {
                let has_todo = queue_file
                    .tasks
                    .iter()
                    .any(|t| t.status == TaskStatus::Todo);
                if has_todo {
                    log::info!("All todo tasks are blocked by unmet dependencies.");
                } else {
                    log::info!("No todo tasks found.");
                }
                return Ok(RunOutcome::NoTodo);
            }
        }
    };

    let task = queue_file.tasks[task_idx].clone();
    let task_id = task.id.trim().to_string();

    // Require clean repo
    gitutil::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        &[".ralph/queue.json", ".ralph/done.json"],
    )?;

    // Mark the task as doing before running the agent.
    mark_task_doing(resolved, &task_id)?;

    // Resolve runner settings
    let settings = resolve_run_agent_settings(resolved, &task, agent_overrides)?;
    let bins = runner::resolve_binaries(&resolved.config.agent);

    log::info!(
        "Executing {task_id}: {title} (runner: {runner:?}, model: {model})",
        title = task.title,
        runner = settings.runner,
        model = settings.model.as_str()
    );

    // --- Prompt Construction ---
    let template = prompts::load_worker_prompt(&resolved.repo_root)?;
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut base_prompt = prompts::render_worker_prompt(&template, project_type, &resolved.config)?;

    // Inject an authoritative task context block to prevent the agent from selecting
    // a different task (e.g., "first todo" or "lowest ID") after Ralph marks the
    // selected task as `doing`.
    let task_context = task_context_for_prompt(&task)?;
    base_prompt = format!("{task_context}\n\n---\n\n{base_prompt}");

    let invocation = phases::PhaseInvocation {
        resolved,
        settings: &settings,
        bins,
        task_id: &task_id,
        base_prompt: &base_prompt,
        policy: &policy,
        output_handler: output_handler.clone(),
        project_type,
    };

    match phases {
        2 => {
            let plan_text = phases::execute_phase1_planning(&invocation, 2)?;
            phases::execute_phase2_implementation(&invocation, 2, &plan_text)?;
        }
        3 => {
            let plan_text = phases::execute_phase1_planning(&invocation, 3)?;
            phases::execute_phase2_implementation(&invocation, 3, &plan_text)?;
            phases::execute_phase3_review(&invocation)?;
        }
        1 => {
            phases::execute_single_phase(&invocation)?;
        }
        _ => unreachable!("phases must be validated to 1..=3"),
    }

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

fn task_context_for_prompt(task: &crate::contracts::Task) -> Result<String> {
    let id = task.id.trim();
    let title = task.title.trim();
    let rendered =
        serde_json::to_string_pretty(task).context("serialize task JSON for prompt context")?;

    Ok(format!(
        r#"# CURRENT TASK (AUTHORITATIVE)

You MUST work on this exact task and no other task.
- Do NOT switch tasks based on queue order, "first todo", or "lowest ID".
- Ignore `.ralph/done.json` except as historical reference if explicitly needed.
- Ralph has already set this task to `doing`. Do NOT change task status manually.

Task ID: {id}
Title: {title}

Raw task JSON (source of truth):
```json
{rendered}
```
"#,
    ))
}

fn mark_task_doing(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let now = timeutil::now_utc_rfc3339()?;
    queue::set_status(&mut queue_file, task_id, TaskStatus::Doing, &now, None)?;
    queue::save_queue(&resolved.queue_path, &queue_file)?;
    Ok(())
}

fn post_run_supervise(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let status = gitutil::status_porcelain(&resolved.repo_root)?;
    let is_dirty = !status.trim().is_empty();

    let mut queue_file = queue::load_queue(&resolved.queue_path)?;
    let mut done_file = queue::load_queue_or_default(&resolved.done_path)?;
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
        warn_if_modified_lfs(&resolved.repo_root);
        if let Err(err) = run_make_ci(&resolved.repo_root) {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!("CI gate failed: 'make ci' did not pass after the task completed. Uncommitted changes were reverted. Fix the issues reported by CI and try again. Error: {:#}", err);
        }

        queue_file = queue::load_queue(&resolved.queue_path)?;
        done_file = queue::load_queue_or_default(&resolved.done_path)?;
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
                bail!("Task inconsistency: task {task_id} is archived in .ralph/done.json but its status is not 'done'. Review the task state in .ralph/done.json.");
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
            &[".ralph/queue.json", ".ralph/done.json"],
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
            bail!("Task inconsistency: task {task_id} is archived in .ralph/done.json but its status is not 'done'. Review the task state in .ralph/done.json.");
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
        &[".ralph/queue.json", ".ralph/done.json"],
    )?;
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
    use crate::completions;
    use crate::contracts::{
        AgentConfig, ClaudePermissionMode, Config, Model, QueueConfig, QueueFile, ReasoningEffort,
        Runner, Task, TaskAgent, TaskStatus,
    };
    use crate::queue;
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
                phases: Some(2),
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
                require_repoprompt: None,
            },
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.json")),
                done_file: Some(PathBuf::from(".ralph/done.json")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(4),
            },
            ..Config::default()
        };

        config::Resolved {
            config: cfg,
            repo_root: repo_root.clone(),
            queue_path: repo_root.join(".ralph/queue.json"),
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.json")),
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

    fn resolved_with_repo_root(repo_root: PathBuf) -> config::Resolved {
        let cfg = Config {
            agent: AgentConfig {
                runner: Some(Runner::Codex),
                model: Some(Model::Gpt52Codex),
                reasoning_effort: Some(ReasoningEffort::Medium),
                codex_bin: Some("codex".to_string()),
                opencode_bin: Some("opencode".to_string()),
                gemini_bin: Some("gemini".to_string()),
                claude_bin: Some("claude".to_string()),
                phases: Some(3),
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
                require_repoprompt: None,
            },
            queue: QueueConfig {
                file: Some(PathBuf::from(".ralph/queue.json")),
                done_file: Some(PathBuf::from(".ralph/done.json")),
                id_prefix: Some("RQ".to_string()),
                id_width: Some(4),
            },
            ..Config::default()
        };

        config::Resolved {
            config: cfg,
            repo_root: repo_root.clone(),
            queue_path: repo_root.join(".ralph/queue.json"),
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.json")),
        }
    }

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
            phases: None,
            repoprompt_required: None,
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
            phases: None,
            repoprompt_required: None,
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
            phases: None,
            repoprompt_required: None,
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
            phases: None,
            repoprompt_required: None,
        };

        let settings = resolve_run_agent_settings(&resolved, &task, &overrides)?;
        assert_eq!(settings.runner, Runner::Opencode);
        assert_eq!(settings.model, Model::Gpt52);
        assert_eq!(settings.reasoning_effort, None);
        Ok(())
    }

    #[test]
    fn task_context_block_includes_id_and_title() -> Result<()> {
        let mut t = base_task();
        t.id = "RQ-0001".to_string();
        t.title = "Hello world".to_string();
        let rendered = task_context_for_prompt(&t)?;
        assert!(rendered.contains("RQ-0001"));
        assert!(rendered.contains("Hello world"));
        assert!(rendered.contains("Raw task JSON"));
        Ok(())
    }

    #[test]
    fn apply_phase3_completion_signal_moves_task_and_clears_signal() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = resolved_with_repo_root(temp.path().to_path_buf());

        let queue_file = QueueFile {
            version: 1,
            tasks: vec![task_with_status(TaskStatus::Doing)],
        };
        queue::save_queue(&resolved.queue_path, &queue_file)?;

        let signal = completions::CompletionSignal {
            task_id: "RQ-0001".to_string(),
            status: TaskStatus::Done,
            notes: vec!["Reviewed".to_string()],
        };
        completions::write_completion_signal(&resolved.repo_root, &signal)?;

        let status = apply_phase3_completion_signal(&resolved, "RQ-0001")?;
        assert_eq!(status, Some(TaskStatus::Done));

        let done = queue::load_queue(&resolved.done_path)?;
        assert_eq!(done.tasks.len(), 1);
        assert_eq!(done.tasks[0].id, "RQ-0001");
        assert_eq!(done.tasks[0].status, TaskStatus::Done);
        assert_eq!(done.tasks[0].notes, vec!["Reviewed".to_string()]);

        let remaining = queue::load_queue(&resolved.queue_path)?;
        assert!(remaining.tasks.is_empty());

        let signal_after = completions::read_completion_signal(&resolved.repo_root, "RQ-0001")?;
        assert!(signal_after.is_none());

        Ok(())
    }

    #[test]
    fn apply_phase3_completion_signal_missing_returns_none() -> Result<()> {
        let temp = TempDir::new()?;
        let resolved = resolved_with_repo_root(temp.path().to_path_buf());

        let queue_file = QueueFile {
            version: 1,
            tasks: vec![task_with_status(TaskStatus::Doing)],
        };
        queue::save_queue(&resolved.queue_path, &queue_file)?;

        let status = apply_phase3_completion_signal(&resolved, "RQ-0001")?;
        assert!(status.is_none());
        Ok(())
    }
}
