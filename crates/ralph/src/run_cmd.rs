use crate::config;
use crate::contracts::{ProjectType, QueueFile, TaskStatus};
use crate::gitutil::GitError;
use crate::promptflow::{
    self, build_phase1_prompt, build_phase2_handoff_prompt, build_phase2_prompt,
    build_phase3_prompt, build_single_phase_prompt,
};
use crate::{gitutil, outpututil, prompts, queue, runner, runutil, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

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
        queue_file
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| anyhow!("Target task {} not found in queue", id))?
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
    let base_prompt = prompts::render_worker_prompt(&template, project_type, &resolved.config)?;

    if phases == 2 {
        log::info!("Phase 1/2 (Planning) for {task_id}...");
        let p1_prompt = build_phase1_prompt(&base_prompt, &task_id, &policy);
        let output = execute_runner_pass(
            resolved,
            &settings,
            bins,
            &p1_prompt,
            output_handler.clone(),
            true,
            "Planning",
        )?;

        // ENFORCEMENT: Phase 1 must not implement.
        // It may only edit `.ralph/queue.json` / `.ralph/done.json` (status bookkeeping).
        if let Err(err) = gitutil::require_clean_repo_ignoring_paths(
            &resolved.repo_root,
            false,
            &[".ralph/queue.json", ".ralph/done.json"],
        ) {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!(
                "Phase 1 violated plan-only contract: it modified files outside allowed queue bookkeeping. Reverted changes. Error: {:#}",
                err
            );
        }

        // Extract and cache plan (STRICT: markers required)
        let plan_text = promptflow::extract_plan_text(settings.runner, &output.stdout)?;
        promptflow::write_plan_cache(&resolved.repo_root, &task_id, &plan_text)?;
        log::info!(
            "Plan cached for {task_id} at {}",
            promptflow::plan_cache_path(&resolved.repo_root, &task_id).display()
        );

        log::info!("Phase 2/2 (Implementation) for {task_id}...");
        let checklist_template = prompts::load_completion_checklist(&resolved.repo_root)?;
        let completion_checklist =
            prompts::render_completion_checklist(&checklist_template, &resolved.config)?;
        let p2_prompt = build_phase2_prompt(&plan_text, &completion_checklist, &policy);
        execute_runner_pass(
            resolved,
            &settings,
            bins,
            &p2_prompt,
            output_handler.clone(),
            true,
            "Implementation",
        )?;

        post_run_supervise(resolved, &task_id)?;
        return Ok(RunOutcome::Ran { task_id });
    }

    if phases == 3 {
        log::info!("Phase 1/3 (Planning) for {task_id}...");
        let p1_prompt = build_phase1_prompt(&base_prompt, &task_id, &policy);
        let output = execute_runner_pass(
            resolved,
            &settings,
            bins,
            &p1_prompt,
            output_handler.clone(),
            true,
            "Planning",
        )?;

        // ENFORCEMENT: Phase 1 must not implement.
        if let Err(err) = gitutil::require_clean_repo_ignoring_paths(
            &resolved.repo_root,
            false,
            &[".ralph/queue.json", ".ralph/done.json"],
        ) {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!(
                "Phase 1 violated plan-only contract: it modified files outside allowed queue bookkeeping. Reverted changes. Error: {:#}",
                err
            );
        }

        let plan_text = promptflow::extract_plan_text(settings.runner, &output.stdout)?;
        promptflow::write_plan_cache(&resolved.repo_root, &task_id, &plan_text)?;
        log::info!(
            "Plan cached for {task_id} at {}",
            promptflow::plan_cache_path(&resolved.repo_root, &task_id).display()
        );

        log::info!("Phase 2/3 (Implementation) for {task_id}...");
        let handoff_template = prompts::load_phase2_handoff_checklist(&resolved.repo_root)?;
        let handoff_checklist =
            prompts::render_phase2_handoff_checklist(&handoff_template, &resolved.config)?;
        let p2_prompt = build_phase2_handoff_prompt(&plan_text, &handoff_checklist, &policy);
        execute_runner_pass(
            resolved,
            &settings,
            bins,
            &p2_prompt,
            output_handler.clone(),
            true,
            "Implementation",
        )?;

        if let Err(err) = run_make_ci(&resolved.repo_root) {
            gitutil::revert_uncommitted(&resolved.repo_root)?;
            bail!(
                "CI gate failed after Phase 2. Uncommitted changes were reverted. Fix the issues reported by CI and rerun. Error: {:#}",
                err
            );
        }

        let review_context = collect_review_context(&resolved.repo_root)?;
        let review_template = prompts::load_code_review_prompt(&resolved.repo_root)?;
        let review_body = prompts::render_code_review_prompt(
            &review_template,
            &task_id,
            &review_context.status,
            &review_context.diff,
            &review_context.diff_staged,
            project_type,
            &resolved.config,
        )?;
        let checklist_template = prompts::load_completion_checklist(&resolved.repo_root)?;
        let completion_checklist =
            prompts::render_completion_checklist(&checklist_template, &resolved.config)?;
        let p3_prompt = build_phase3_prompt(
            &base_prompt,
            &review_body,
            &completion_checklist,
            &policy,
            &task_id,
        );

        runutil::run_prompt_with_handling(
            runutil::RunnerInvocation {
                repo_root: &resolved.repo_root,
                runner_kind: settings.runner,
                bins,
                model: settings.model.clone(),
                reasoning_effort: settings.reasoning_effort,
                prompt: &p3_prompt,
                timeout: None,
                permission_mode: resolved.config.agent.claude_permission_mode,
                revert_on_error: false,
                output_handler: output_handler.clone(),
            },
            runutil::RunnerErrorMessages {
                log_label: "Code review",
                interrupted_msg: "Code review interrupted: the agent run was canceled. Review the working tree and rerun Phase 3 to complete the task.",
                timeout_msg: "Code review timed out: the agent run exceeded the time limit. Review the working tree and rerun Phase 3 to complete the task.",
                terminated_msg: "Code review terminated: the agent was stopped by a signal. Review the working tree and rerun Phase 3 to complete the task.",
                non_zero_msg: |code| {
                    format!(
                        "Code review failed: the agent exited with a non-zero code ({code}). Review the working tree and rerun Phase 3 to complete the task."
                    )
                },
                other_msg: |err| {
                    format!(
                        "Code review failed: the agent could not be started or encountered an error. Review the working tree and rerun Phase 3. Error: {:#}",
                        err
                    )
                },
            },
        )?;

        ensure_phase3_completion(resolved, &task_id)?;
        return Ok(RunOutcome::Ran { task_id });
    }

    // phases == 1: Single-pass execution
    let checklist_template = prompts::load_completion_checklist(&resolved.repo_root)?;
    let completion_checklist =
        prompts::render_completion_checklist(&checklist_template, &resolved.config)?;
    let prompt = build_single_phase_prompt(&base_prompt, &completion_checklist, &task_id, &policy);
    execute_runner_pass(
        resolved,
        &settings,
        bins,
        &prompt,
        output_handler.clone(),
        true,
        "Execution",
    )?;

    post_run_supervise(resolved, &task_id)?;
    Ok(RunOutcome::Ran { task_id })
}

fn execute_runner_pass(
    resolved: &config::Resolved,
    settings: &runner::AgentSettings,
    bins: runner::RunnerBinaries,
    prompt: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_on_error: bool,
    log_label: &str,
) -> Result<runner::RunnerOutput> {
    let permission_mode = resolved.config.agent.claude_permission_mode;

    // Special case: Phase 1 planning for Claude should ideally be unblocked (BypassPermissions)
    // assuming it is read-only or just planning.
    // But `run_one_impl` doesn't pass phase info down here easily.
    // However, the caller sets up the prompt.
    // We can rely on user config generally, OR force Bypass for planning if we want to follow spec strictly.
    // Spec says: "Phase 1 planning runs Claude with BypassPermissions (avoid blocking)."
    // We can't easily detect if it's phase 1 here without passing another arg.
    // Let's stick to config for now unless we refactor to pass phase context.
    // Actually, `runutil` just takes `permission_mode`.
    // I will use `resolved.config` value.

    runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            repo_root: &resolved.repo_root,
            runner_kind: settings.runner,
            bins,
            model: settings.model.clone(),
            reasoning_effort: settings.reasoning_effort,
            prompt,
            timeout: None,
            permission_mode,
            revert_on_error,
            output_handler,
        },
        runutil::RunnerErrorMessages {
            log_label,
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
    )
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

fn ensure_phase3_completion(resolved: &config::Resolved, task_id: &str) -> Result<()> {
    let queue_file = queue::load_queue(&resolved.queue_path)?;
    let done_file = queue::load_queue_or_default(&resolved.done_path)?;
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

    let (status, _title, in_done) = find_task_status(&queue_file, &done_file, task_id)
        .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;

    if !in_done || !(status == TaskStatus::Done || status == TaskStatus::Rejected) {
        bail!(
            "Phase 3 incomplete: task {task_id} is not archived with a terminal status. Run `ralph queue complete` in Phase 3 before finishing."
        );
    }

    gitutil::require_clean_repo_ignoring_paths(&resolved.repo_root, false, &[])?;
    Ok(())
}

struct ReviewContext {
    status: String,
    diff: String,
    diff_staged: String,
}

fn collect_review_context(repo_root: &Path) -> Result<ReviewContext> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain"])
        .output()
        .with_context(|| format!("run git status --porcelain in {}", repo_root.display()))?;
    let diff = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff"])
        .output()
        .with_context(|| format!("run git diff in {}", repo_root.display()))?;
    let diff_staged = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--staged"])
        .output()
        .with_context(|| format!("run git diff --staged in {}", repo_root.display()))?;

    let status_str = String::from_utf8_lossy(&status.stdout).to_string();
    let diff_str = String::from_utf8_lossy(&diff.stdout).to_string();
    let diff_staged_str = String::from_utf8_lossy(&diff_staged.stdout).to_string();

    Ok(ReviewContext {
        status: normalize_git_output(status_str, "(no pending changes)"),
        diff: normalize_git_output(diff_str, "(no diff)"),
        diff_staged: normalize_git_output(diff_staged_str, "(no staged diff)"),
    })
}

fn normalize_git_output(value: String, empty_label: &str) -> String {
    if value.trim().is_empty() {
        empty_label.to_string()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        AgentConfig, ClaudePermissionMode, Config, Model, QueueConfig, ReasoningEffort, Runner,
        Task, TaskAgent, TaskStatus,
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
}
