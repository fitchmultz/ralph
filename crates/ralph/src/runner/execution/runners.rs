//! Runner-specific command assembly for execution.
//!
//! Responsibilities:
//! - Assemble runner-specific commands and payloads for execution.
//! - Delegate execution to the shared streaming process runner.
//!
//! Does not handle:
//! - Validating models or global runner configuration.
//! - Persisting runner output or mutating queue state.
//!
//! Assumptions/invariants:
//! - Caller supplies validated model/runner inputs and a writable work dir.
//! - Command builder guards are kept alive for the duration of execution.

use std::path::Path;
use std::time::Duration;

use super::super::{
    runner_execution_error_with_source, ClaudePermissionMode, Model, OutputHandler, OutputStream,
    ReasoningEffort, RunnerError, RunnerOutput,
};
use super::cli_options::ResolvedRunnerCliOptions;
use super::cli_spec;
use super::command::RunnerCommandBuilder;
use super::process::run_with_streaming_json;
use crate::commands::run::PhaseType;
use crate::contracts::Runner;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_codex(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_codex_global_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("exec")
        .legacy_json_format()
        .model(&model)
        .reasoning_effort(reasoning_effort)
        .arg("-")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Codex,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_codex_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    thread_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_codex_global_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("exec")
        .arg("resume")
        .arg(thread_id)
        .legacy_json_format()
        .model(&model)
        .reasoning_effort(reasoning_effort)
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Codex,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_opencode(
    work_dir: &Path,
    bin: &str,
    _runner_cli: ResolvedRunnerCliOptions,
    model: &Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .arg("run")
        .model(model)
        .opencode_format()
        .with_temp_prompt_file(prompt)
        .map_err(|err| {
            runner_execution_error_with_source(
                Runner::Opencode,
                bin,
                "create temp prompt file",
                err,
            )
        })?
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Opencode,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_opencode_resume(
    work_dir: &Path,
    bin: &str,
    _runner_cli: ResolvedRunnerCliOptions,
    model: &Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .arg("run")
        .arg("-s")
        .arg(session_id)
        .model(model)
        .opencode_format()
        .arg("--")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Opencode,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_gemini(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_gemini_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .model(&model)
        .output_format("stream-json")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Gemini,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_gemini_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_gemini_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .output_format("stream-json")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Gemini,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_claude(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_claude_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("-p")
        .model(&model)
        .permission_mode(permission_mode)
        .output_format("stream-json")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Claude,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_claude_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_claude_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .permission_mode(permission_mode)
        .output_format("stream-json")
        .arg("-p")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Claude,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_kimi(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_kimi_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .model(&model)
        .output_format("stream-json")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Kimi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_kimi_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_kimi_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .output_format("stream-json")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Kimi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_pi(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_pi_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .model(&model)
        .output_format("stream-json")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Pi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_pi_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir);
    let builder = cli_spec::apply_pi_options(builder, runner_cli);
    let (cmd, payload, _guards) = builder
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .output_format("stream-json")
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Pi,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_cursor(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    phase_type: PhaseType,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir).model(&model);
    let builder = cli_spec::apply_cursor_options(builder, runner_cli, phase_type);

    let (cmd, payload, _guards) = builder
        .arg("--print")
        .output_format("stream-json")
        // Cursor agent CLI expects the prompt as a positional argument.
        .arg(prompt)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Cursor,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_cursor_resume(
    work_dir: &Path,
    bin: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    phase_type: PhaseType,
) -> Result<RunnerOutput, RunnerError> {
    let builder = RunnerCommandBuilder::new(bin, work_dir)
        .arg("--resume")
        .arg(session_id)
        .model(&model);
    let builder = cli_spec::apply_cursor_options(builder, runner_cli, phase_type);

    let (cmd, payload, _guards) = builder
        .arg("--print")
        .output_format("stream-json")
        // Cursor agent CLI expects the continuation message as a positional argument.
        .arg(message)
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        Runner::Cursor,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}
