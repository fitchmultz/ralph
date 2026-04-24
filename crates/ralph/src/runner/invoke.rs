//! Unified runner invocation dispatch for prompt and resume operations.
//!
//! Purpose:
//! - Unified runner invocation dispatch for prompt and resume operations.
//!
//! Responsibilities:
//! - Centralize built-in runner invocation/resume validation and dispatch.
//! - Share exit-status and semantic-failure handling across operations.
//! - Keep `runner.rs` as a thin public facade over a cohesive dispatch model.
//!
//! Non-scope:
//! - Plugin registry lookup for external plugin runners (see `plugin_dispatch`).
//! - Runner-specific command construction (see `crate::runner::execution`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Built-in runner binaries are resolved before dispatch.
//! - Model validation occurs before execution for every operation.

use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, Model, ReasoningEffort, Runner};
use crate::plugins::registry::PluginRegistry;
use crate::runner::{
    OutputHandler, OutputStream, RunnerBinaries, RunnerError, RunnerOutput, execution,
    runner_label, validate_model_for_runner,
};
use anyhow::{Result, anyhow};
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;

#[derive(Clone)]
pub(crate) enum RunnerInvocation<'a> {
    Prompt {
        prompt: &'a str,
        session_id: Option<String>,
    },
    Resume {
        session_id: &'a str,
        message: &'a str,
    },
}

pub(crate) struct RunnerDispatchContext<'a> {
    pub runner: Runner,
    pub work_dir: &'a Path,
    pub bins: RunnerBinaries<'a>,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: execution::ResolvedRunnerCliOptions,
    pub timeout: Option<Duration>,
    pub permission_mode: Option<ClaudePermissionMode>,
    pub output_handler: Option<OutputHandler>,
    pub output_stream: OutputStream,
    pub phase_type: PhaseType,
    pub plugins: Option<&'a PluginRegistry>,
}

pub(crate) fn dispatch(
    ctx: RunnerDispatchContext<'_>,
    invocation: RunnerInvocation<'_>,
) -> Result<RunnerOutput, RunnerError> {
    let operation = invocation.operation_name();
    validate_model(&ctx, operation)?;

    if let Runner::Plugin(plugin_id) = &ctx.runner {
        return crate::runner::plugin_dispatch::dispatch_plugin_operation(
            plugin_id,
            ctx.work_dir,
            ctx.runner_cli,
            ctx.model,
            ctx.timeout,
            ctx.output_handler,
            ctx.output_stream,
            invocation,
            ctx.plugins,
        );
    }

    let executor = execution::PluginExecutor::new();
    let output = match invocation {
        RunnerInvocation::Prompt { prompt, session_id } => executor.run(
            ctx.runner.clone(),
            ctx.work_dir,
            resolve_bin(&ctx.runner, ctx.bins),
            ctx.model,
            ctx.reasoning_effort,
            ctx.runner_cli,
            prompt,
            ctx.timeout,
            effective_permission_mode(&ctx.runner, &ctx.runner_cli, ctx.permission_mode),
            ctx.output_handler,
            ctx.output_stream,
            ctx.phase_type,
            session_id,
            ctx.plugins,
        )?,
        RunnerInvocation::Resume {
            session_id,
            message,
        } => {
            validate_resume_inputs(
                &ctx.runner,
                resolve_bin(&ctx.runner, ctx.bins),
                session_id,
                message,
            )?;
            executor.resume(
                ctx.runner.clone(),
                ctx.work_dir,
                resolve_bin(&ctx.runner, ctx.bins),
                ctx.model,
                ctx.reasoning_effort,
                ctx.runner_cli,
                session_id,
                message,
                ctx.timeout,
                effective_permission_mode(&ctx.runner, &ctx.runner_cli, ctx.permission_mode),
                ctx.output_handler,
                ctx.output_stream,
                ctx.phase_type,
                ctx.plugins,
            )?
        }
    };

    finalize_output(
        operation,
        &ctx.runner,
        resolve_bin(&ctx.runner, ctx.bins),
        output,
    )
}

fn validate_model(ctx: &RunnerDispatchContext<'_>, operation: &str) -> Result<(), RunnerError> {
    let bin = resolve_bin(&ctx.runner, ctx.bins);
    validate_model_for_runner(&ctx.runner, &ctx.model).map_err(|err| {
        RunnerError::Other(anyhow!(
            "Runner configuration error (operation={}, runner={}, bin={}): {}",
            operation,
            runner_label(ctx.runner.clone()),
            bin,
            err
        ))
    })
}

fn validate_resume_inputs(
    runner: &Runner,
    bin: &str,
    session_id: &str,
    message: &str,
) -> Result<(), RunnerError> {
    let session_id = session_id.trim();
    if runner_requires_session_id(runner) && session_id.is_empty() {
        return Err(RunnerError::Other(anyhow!(
            "Runner input error (operation=resume_session, runner={}, bin={}): session_id is required (non-empty). Example: --resume <SESSION_ID>.",
            runner_label(runner.clone()),
            bin
        )));
    }

    if message.trim().is_empty() {
        return Err(RunnerError::Other(anyhow!(
            "Runner input error (operation=resume_session, runner={}, bin={}): message is required (non-empty).",
            runner_label(runner.clone()),
            bin
        )));
    }

    Ok(())
}

fn effective_permission_mode(
    runner: &Runner,
    runner_cli: &execution::ResolvedRunnerCliOptions,
    permission_mode: Option<ClaudePermissionMode>,
) -> Option<ClaudePermissionMode> {
    match runner {
        Runner::Claude => runner_cli.effective_claude_permission_mode(permission_mode),
        _ => None,
    }
}

fn finalize_output(
    operation: &str,
    runner: &Runner,
    bin: &str,
    output: RunnerOutput,
) -> Result<RunnerOutput, RunnerError> {
    if !output.status.success() {
        return Err(map_exit_status(
            output.status,
            output.stdout,
            output.stderr,
            output.session_id,
        ));
    }

    if let Some(reason) = semantic_failure_reason(runner, &output.stderr) {
        return Err(semantic_failure_error(operation, runner, bin, reason));
    }

    Ok(output)
}

fn map_exit_status(
    status: ExitStatus,
    stdout: String,
    stderr: String,
    session_id: Option<String>,
) -> RunnerError {
    if let Some(code) = status.code() {
        RunnerError::NonZeroExit {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
            session_id,
        }
    } else {
        RunnerError::TerminatedBySignal {
            signal: exit_status_signal(&status),
            stdout: stdout.into(),
            stderr: stderr.into(),
            session_id,
        }
    }
}

fn resolve_bin<'a>(runner: &Runner, bins: RunnerBinaries<'a>) -> &'a str {
    match runner {
        Runner::Codex => bins.codex,
        Runner::Opencode => bins.opencode,
        Runner::Gemini => bins.gemini,
        Runner::Cursor => bins.cursor,
        Runner::Claude => bins.claude,
        Runner::Kimi => bins.kimi,
        Runner::Pi => bins.pi,
        Runner::Plugin(_) => unreachable!("plugin bins resolved via registry"),
    }
}

fn runner_requires_session_id(_runner: &Runner) -> bool {
    true
}

pub(crate) fn semantic_failure_reason(runner: &Runner, stderr: &str) -> Option<&'static str> {
    match runner {
        Runner::Opencode => {
            let stderr_lower = stderr.to_ascii_lowercase();
            let has_zod_session_validation_error = stderr_lower.contains("zoderror")
                && stderr_lower.contains("sessionid")
                && ((stderr_lower.contains("must start with") && stderr_lower.contains("ses"))
                    || stderr_lower.contains("invalid_format"));

            if has_zod_session_validation_error {
                Some("opencode rejected session_id during resume validation")
            } else {
                None
            }
        }
        _ => None,
    }
}

fn semantic_failure_error(
    operation: &str,
    runner: &Runner,
    bin: &str,
    reason: &str,
) -> RunnerError {
    RunnerError::Other(anyhow!(
        "Runner execution failed (operation={}, runner={}, bin={}): semantic failure with zero exit status: {}.",
        operation,
        runner_label(runner.clone()),
        bin,
        reason
    ))
}

#[cfg(unix)]
fn exit_status_signal(status: &ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn exit_status_signal(_status: &ExitStatus) -> Option<i32> {
    None
}

impl RunnerInvocation<'_> {
    fn operation_name(&self) -> &'static str {
        match self {
            Self::Prompt { .. } => "run_prompt",
            Self::Resume { .. } => "resume_session",
        }
    }
}
