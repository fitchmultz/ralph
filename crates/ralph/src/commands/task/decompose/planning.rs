//! Planner prompt execution and response parsing for task decomposition.
//!
//! Responsibilities:
//! - Load the decomposition prompt template and invoke the configured runner.
//! - Resolve source/attach metadata into preview inputs before planner execution.
//! - Parse planner JSON output into normalized preview trees and write blockers.
//!
//! Not handled here:
//! - Queue mutation or undo snapshot creation.
//! - Tree normalization internals or task materialization helpers.
//!
//! Invariants/assumptions:
//! - Planner output must contain a final JSON object matching the decomposition schema.
//! - Preview planning remains read-only with respect to queue/done files.

use super::super::resolve_task_runner_settings;
use super::resolve::{compute_write_blockers, resolve_attach_target, resolve_source};
use super::support::kind_for_source;
use super::tree::normalize_response;
use super::types::{
    DecompositionAttachTarget, DecompositionPreview, DecompositionSource, RawDecompositionResponse,
    TaskDecomposeOptions,
};
use crate::commands::run::PhaseType;
use crate::contracts::ProjectType;
use crate::{config, prompts, queue, runner, runutil};
use anyhow::{Context, Result};

pub fn plan_task_decomposition(
    resolved: &config::Resolved,
    opts: &TaskDecomposeOptions,
) -> Result<DecompositionPreview> {
    let (active, done) = queue::load_and_validate_queues(resolved, true)?;
    let source = resolve_source(resolved, &active, done.as_ref(), opts.source_input.trim())?;
    let attach_target = resolve_attach_target(
        resolved,
        &active,
        done.as_ref(),
        opts.attach_to_task_id.as_deref(),
        &source,
    )?;

    let template = prompts::load_task_decompose_prompt(&resolved.repo_root)?;
    let prompt = build_planner_prompt(resolved, opts, &source, attach_target.as_ref(), &template)?;
    let settings = resolve_task_runner_settings(
        resolved,
        opts.runner_override.clone(),
        opts.model_override.clone(),
        opts.reasoning_effort_override,
        &opts.runner_cli_overrides,
    )?;
    let bins = runner::resolve_binaries(&resolved.config.agent);
    let retry_policy = runutil::RunnerRetryPolicy::from_config(&resolved.config.agent.runner_retry)
        .unwrap_or_default();

    let output = runutil::run_prompt_with_handling(
        runutil::RunnerInvocation {
            settings: runutil::RunnerSettings {
                repo_root: &resolved.repo_root,
                runner_kind: settings.runner,
                bins,
                model: settings.model,
                reasoning_effort: settings.reasoning_effort,
                runner_cli: settings.runner_cli,
                timeout: None,
                permission_mode: settings.permission_mode,
                output_handler: None,
                output_stream: runner::OutputStream::Terminal,
            },
            execution: runutil::RunnerExecutionContext {
                prompt: &prompt,
                phase_type: PhaseType::SinglePhase,
                session_id: None,
            },
            failure: runutil::RunnerFailureHandling {
                revert_on_error: false,
                git_revert_mode: resolved
                    .config
                    .agent
                    .git_revert_mode
                    .unwrap_or(crate::contracts::GitRevertMode::Ask),
                revert_prompt: None,
            },
            retry: runutil::RunnerRetryState {
                policy: retry_policy,
            },
        },
        runutil::RunnerErrorMessages {
            log_label: "task decompose planner",
            interrupted_msg: "Task decomposition interrupted: the planner run was canceled.",
            timeout_msg: "Task decomposition timed out before a plan was returned.",
            terminated_msg: "Task decomposition terminated: the planner was stopped by a signal.",
            non_zero_msg: |code| {
                format!(
                    "Task decomposition failed: the planner exited with a non-zero code ({code})."
                )
            },
            other_msg: |err| {
                format!(
                    "Task decomposition failed: the planner could not be started or encountered an error. Error: {:#}",
                    err
                )
            },
        },
    )?;

    let planner_text = extract_planner_text(&output.stdout).context(
        "Task decomposition planner did not produce a final assistant response containing JSON.",
    )?;
    let raw = parse_planner_response(&planner_text)?;
    let default_root_title = match &source {
        DecompositionSource::Freeform { request } => request.clone(),
        DecompositionSource::ExistingTask { task } => task.title.clone(),
    };
    let plan = normalize_response(raw, kind_for_source(&source), opts, &default_root_title)?;
    let write_blockers = compute_write_blockers(
        &active,
        done.as_ref(),
        &source,
        attach_target.as_ref(),
        opts.child_policy,
    )?;

    Ok(DecompositionPreview {
        source,
        attach_target,
        plan,
        write_blockers,
        child_status: opts.status,
        child_policy: opts.child_policy,
        with_dependencies: opts.with_dependencies,
    })
}

fn build_planner_prompt(
    resolved: &config::Resolved,
    opts: &TaskDecomposeOptions,
    source: &DecompositionSource,
    attach_target: Option<&DecompositionAttachTarget>,
    template: &str,
) -> Result<String> {
    let (source_mode, source_request, source_task_json) = match source {
        DecompositionSource::Freeform { request } => ("freeform", request.clone(), String::new()),
        DecompositionSource::ExistingTask { task } => (
            "existing_task",
            task.request.clone().unwrap_or_else(|| task.title.clone()),
            serde_json::to_string_pretty(task)
                .context("serialize source task for decomposition")?,
        ),
    };
    let attach_target_json = attach_target
        .map(|target| {
            serde_json::to_string_pretty(&target.task)
                .context("serialize attach target for decomposition")
        })
        .transpose()?
        .unwrap_or_default();
    let project_type = resolved.config.project_type.unwrap_or(ProjectType::Code);
    let mut prompt = prompts::render_task_decompose_prompt(
        template,
        source_mode,
        &source_request,
        &source_task_json,
        &attach_target_json,
        opts.max_depth,
        opts.max_children,
        opts.max_nodes,
        opts.child_policy,
        opts.with_dependencies,
        project_type,
        &resolved.config,
    )?;
    prompt = prompts::wrap_with_repoprompt_requirement(&prompt, opts.repoprompt_tool_injection);
    prompts::wrap_with_instruction_files(&resolved.repo_root, &prompt, &resolved.config)
}

fn extract_planner_text(stdout: &str) -> Option<String> {
    runner::extract_final_assistant_response(stdout).or_else(|| {
        let trimmed = stdout.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn parse_planner_response(raw_text: &str) -> Result<RawDecompositionResponse> {
    let stripped = strip_code_fences(raw_text.trim());
    serde_json::from_str::<RawDecompositionResponse>(stripped)
        .or_else(|_| match extract_json_object(stripped) {
            Some(candidate) => serde_json::from_str::<RawDecompositionResponse>(&candidate),
            None => Err(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "no JSON object found in planner response",
            ))),
        })
        .context("parse task decomposition planner JSON")
}

fn strip_code_fences(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix("```")
        && let Some(end) = inner.rfind("```")
    {
        let body = &inner[..end];
        if let Some(after_language) = body.find('\n') {
            return body[after_language + 1..].trim();
        }
        return body.trim();
    }
    trimmed
}

fn extract_json_object(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (start < end).then(|| raw[start..=end].to_string())
}
