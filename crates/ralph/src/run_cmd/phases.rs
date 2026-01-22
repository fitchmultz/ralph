//! Phase-specific execution logic for `ralph run`.
//!
//! This module isolates multi-phase runner workflows (planning, implementation,
//! code review) from higher-level orchestration in `crate::run_cmd`.

use crate::completions;
use crate::config;
use crate::contracts::{ProjectType, TaskStatus};
use crate::{gitutil, promptflow, prompts, queue, runner, runutil, timeutil};
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;
use std::process::Command;

use super::logging;

/// Shared inputs for executing a run phase workflow.
///
/// This struct intentionally groups parameters to keep function signatures small and
/// avoid clippy `too_many_arguments`, while preserving exact behaviors from
/// `run_cmd.rs`.
#[derive(Clone)]
pub struct PhaseInvocation<'a> {
    pub resolved: &'a config::Resolved,
    pub settings: &'a runner::AgentSettings,
    pub bins: runner::RunnerBinaries<'a>,
    pub task_id: &'a str,
    pub base_prompt: &'a str,
    pub policy: &'a promptflow::PromptPolicy,
    pub output_handler: Option<runner::OutputHandler>,
    pub project_type: ProjectType,
    pub git_revert_mode: crate::contracts::GitRevertMode,
}

pub struct ReviewContext {
    pub status: String,
    pub diff: String,
    pub diff_staged: String,
}

pub fn execute_phase1_planning(ctx: &PhaseInvocation<'_>, total_phases: u8) -> Result<String> {
    let label = logging::phase_label(1, total_phases, "Planning", ctx.task_id);

    logging::with_scope(&label, || {
        let p1_prompt = promptflow::build_phase1_prompt(ctx.base_prompt, ctx.task_id, ctx.policy);
        let _output = execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &p1_prompt,
            ctx.output_handler.clone(),
            true,
            ctx.git_revert_mode,
            "Planning",
        )?;

        // ENFORCEMENT: Phase 1 must not implement.
        // It may only edit `.ralph/queue.json` / `.ralph/done.json` (status bookkeeping)
        // plus the plan cache file for the current task.
        let plan_cache_rel = format!(".ralph/cache/plans/{}.md", ctx.task_id);
        let allowed_paths = [
            ".ralph/queue.json",
            ".ralph/done.json",
            plan_cache_rel.as_str(),
        ];
        if let Err(err) = gitutil::require_clean_repo_ignoring_paths(
            &ctx.resolved.repo_root,
            false,
            &allowed_paths,
        ) {
            let outcome = runutil::apply_git_revert_mode(
                &ctx.resolved.repo_root,
                ctx.git_revert_mode,
                "Phase 1 plan-only violation",
            )?;
            bail!(
                "{} Error: {:#}",
                runutil::format_revert_failure_message(
                    "Phase 1 violated plan-only contract: it modified files outside allowed queue bookkeeping.",
                    outcome,
                ),
                err
            );
        }

        // Read plan from cache (Phase 1 writes it directly).
        let plan_text = promptflow::read_plan_cache(&ctx.resolved.repo_root, ctx.task_id)?;
        log::info!(
            "Plan cached for {} at {}",
            ctx.task_id,
            promptflow::plan_cache_path(&ctx.resolved.repo_root, ctx.task_id).display()
        );

        Ok(plan_text)
    })
}

pub fn execute_phase2_implementation(
    ctx: &PhaseInvocation<'_>,
    total_phases: u8,
    plan_text: &str,
) -> Result<()> {
    let label = logging::phase_label(2, total_phases, "Implementation", ctx.task_id);

    logging::with_scope(&label, || {
        if total_phases == 3 {
            let handoff_template = prompts::load_phase2_handoff_checklist(&ctx.resolved.repo_root)?;
            let handoff_checklist =
                prompts::render_phase2_handoff_checklist(&handoff_template, &ctx.resolved.config)?;
            let p2_prompt =
                promptflow::build_phase2_handoff_prompt(plan_text, &handoff_checklist, ctx.policy);

            execute_runner_pass(
                ctx.resolved,
                ctx.settings,
                ctx.bins,
                &p2_prompt,
                ctx.output_handler.clone(),
                true,
                ctx.git_revert_mode,
                "Implementation",
            )?;

            if let Err(err) = super::run_make_ci(&ctx.resolved.repo_root) {
                let outcome = runutil::apply_git_revert_mode(
                    &ctx.resolved.repo_root,
                    ctx.git_revert_mode,
                    "Phase 2 CI failure",
                )?;
                bail!(
                    "{} Error: {:#}",
                    runutil::format_revert_failure_message(
                        "CI gate failed after Phase 2. Fix issues reported by CI and rerun.",
                        outcome,
                    ),
                    err
                );
            }

            return Ok(());
        }

        let checklist_template = prompts::load_completion_checklist(&ctx.resolved.repo_root)?;
        let completion_checklist =
            prompts::render_completion_checklist(&checklist_template, &ctx.resolved.config)?;
        let p2_prompt =
            promptflow::build_phase2_prompt(plan_text, &completion_checklist, ctx.policy);

        execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &p2_prompt,
            ctx.output_handler.clone(),
            true,
            ctx.git_revert_mode,
            "Implementation",
        )?;

        super::post_run_supervise(ctx.resolved, ctx.task_id, ctx.git_revert_mode)?;
        Ok(())
    })
}

pub fn execute_phase3_review(ctx: &PhaseInvocation<'_>) -> Result<()> {
    let label = logging::phase_label(3, 3, "Review", ctx.task_id);

    logging::with_scope(&label, || {
        let review_context = collect_review_context(&ctx.resolved.repo_root)?;
        let review_template = prompts::load_code_review_prompt(&ctx.resolved.repo_root)?;
        let review_body = prompts::render_code_review_prompt(
            &review_template,
            ctx.task_id,
            &review_context.status,
            &review_context.diff,
            &review_context.diff_staged,
            ctx.project_type,
            &ctx.resolved.config,
        )?;

        let checklist_template = prompts::load_completion_checklist(&ctx.resolved.repo_root)?;
        let completion_checklist =
            prompts::render_completion_checklist(&checklist_template, &ctx.resolved.config)?;
        let p3_prompt = promptflow::build_phase3_prompt(
            ctx.base_prompt,
            &review_body,
            &completion_checklist,
            ctx.policy,
            ctx.task_id,
        );

        runutil::run_prompt_with_handling(
            runutil::RunnerInvocation {
                repo_root: &ctx.resolved.repo_root,
                runner_kind: ctx.settings.runner,
                bins: ctx.bins,
                model: ctx.settings.model.clone(),
                reasoning_effort: ctx.settings.reasoning_effort,
            prompt: &p3_prompt,
            timeout: None,
            permission_mode: ctx.resolved.config.agent.claude_permission_mode,
            revert_on_error: false,
            git_revert_mode: ctx.git_revert_mode,
            output_handler: ctx.output_handler.clone(),
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

        if let Some(status) = apply_phase3_completion_signal(ctx.resolved, ctx.task_id)? {
            if status == TaskStatus::Done {
                super::post_run_supervise(ctx.resolved, ctx.task_id, ctx.git_revert_mode)?;
            }
        }

        ensure_phase3_completion(ctx.resolved, ctx.task_id)?;
        Ok(())
    })
}

pub fn execute_single_phase(ctx: &PhaseInvocation<'_>) -> Result<()> {
    let label = logging::single_phase_label("SinglePhase (Execution)", ctx.task_id);

    logging::with_scope(&label, || {
        let checklist_template = prompts::load_completion_checklist(&ctx.resolved.repo_root)?;
        let completion_checklist =
            prompts::render_completion_checklist(&checklist_template, &ctx.resolved.config)?;
        let prompt = promptflow::build_single_phase_prompt(
            ctx.base_prompt,
            &completion_checklist,
            ctx.task_id,
            ctx.policy,
        );

        execute_runner_pass(
            ctx.resolved,
            ctx.settings,
            ctx.bins,
            &prompt,
            ctx.output_handler.clone(),
            true,
            ctx.git_revert_mode,
            "Execution",
        )?;

        super::post_run_supervise(ctx.resolved, ctx.task_id, ctx.git_revert_mode)?;
        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
pub fn execute_runner_pass(
    resolved: &config::Resolved,
    settings: &runner::AgentSettings,
    bins: runner::RunnerBinaries,
    prompt: &str,
    output_handler: Option<runner::OutputHandler>,
    revert_on_error: bool,
    git_revert_mode: crate::contracts::GitRevertMode,
    log_label: &str,
) -> Result<runner::RunnerOutput> {
    let permission_mode = resolved.config.agent.claude_permission_mode;

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
            git_revert_mode,
            output_handler,
        },
        runutil::RunnerErrorMessages {
            log_label,
            interrupted_msg: "Runner interrupted: the execution was canceled by the user or system.",
            timeout_msg: "Runner timed out: the execution exceeded the allowed time limit. Changes in the working tree were NOT reverted; review the repo state manually.",
            terminated_msg: "Runner terminated: the agent was stopped by a signal. Rerunning the task is recommended.",
            non_zero_msg: |code| {
                format!(
                    "Runner failed: the agent exited with a non-zero code ({code}). Rerunning the task is recommended after investigating the cause."
                )
            },
            other_msg: |err| {
                format!(
                    "Runner invocation failed: the agent could not be started or encountered an error. Rerunning the task is recommended. Error: {:#}",
                    err
                )
            },
        },
    )
}

pub fn apply_phase3_completion_signal(
    resolved: &config::Resolved,
    task_id: &str,
) -> Result<Option<TaskStatus>> {
    let Some(signal) = completions::take_completion_signal(&resolved.repo_root, task_id)? else {
        return Ok(None);
    };

    let now = timeutil::now_utc_rfc3339()?;
    let status = signal.status;
    queue::complete_task(
        &resolved.queue_path,
        &resolved.done_path,
        task_id,
        status,
        &now,
        &signal.notes,
        &resolved.id_prefix,
        resolved.id_width,
    )?;
    log::info!(
        "Supervisor finalized task {} with status {:?} from Phase 3 completion signal.",
        task_id,
        status
    );
    Ok(Some(status))
}

pub fn ensure_phase3_completion(resolved: &config::Resolved, task_id: &str) -> Result<()> {
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

    let (status, _title, in_done) = super::find_task_status(&queue_file, &done_file, task_id)
        .ok_or_else(|| anyhow!("task {task_id} not found in queue or done"))?;

    if !in_done || !(status == TaskStatus::Done || status == TaskStatus::Rejected) {
        bail!(
            "Phase 3 incomplete: task {task_id} is not archived with a terminal status. Run `ralph task done` in Phase 3 before finishing."
        );
    }

    gitutil::require_clean_repo_ignoring_paths(&resolved.repo_root, false, &[])?;
    Ok(())
}

pub fn collect_review_context(repo_root: &Path) -> Result<ReviewContext> {
    let status = Command::new("git")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain"])
        .output()
        .with_context(|| format!("run git status --porcelain in {}", repo_root.display()))?;
    let diff = Command::new("git")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-C")
        .arg(repo_root)
        .args(["diff"])
        .output()
        .with_context(|| format!("run git diff in {}", repo_root.display()))?;
    let diff_staged = Command::new("git")
        .arg("-c")
        .arg("core.fsmonitor=false")
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
