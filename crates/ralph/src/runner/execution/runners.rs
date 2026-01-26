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
use super::command::RunnerCommandBuilder;
use super::process::run_with_streaming_json;
use crate::contracts::Runner;

#[allow(clippy::too_many_arguments)]
pub fn run_codex(
    work_dir: &Path,
    bin: &str,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
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
pub fn run_codex_resume(
    work_dir: &Path,
    bin: &str,
    model: Model,
    reasoning_effort: Option<ReasoningEffort>,
    thread_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
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

pub fn run_opencode(
    work_dir: &Path,
    bin: &str,
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
pub fn run_opencode_resume(
    work_dir: &Path,
    bin: &str,
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

pub fn run_gemini(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .model(&model)
        .output_format("stream-json")
        .arg("--approval-mode")
        .arg("yolo")
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
pub fn run_gemini_resume(
    work_dir: &Path,
    bin: &str,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .output_format("stream-json")
        .arg("--approval-mode")
        .arg("yolo")
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
pub fn run_claude(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .arg("-p")
        .model(&model)
        .permission_mode(permission_mode)
        .output_format("stream-json")
        .arg("--verbose")
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
pub fn run_claude_resume(
    work_dir: &Path,
    bin: &str,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    permission_mode: Option<ClaudePermissionMode>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    let (cmd, payload, _guards) = RunnerCommandBuilder::new(bin, work_dir)
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .permission_mode(permission_mode)
        .output_format("stream-json")
        .arg("--verbose")
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

fn cursor_is_planning(text: &str) -> bool {
    text.contains("# PLANNING MODE")
}

#[allow(clippy::too_many_arguments)]
pub fn run_cursor(
    work_dir: &Path,
    bin: &str,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    // Phase detection is intentionally string-based to avoid refactors:
    // Phase 1 prompts include "# PLANNING MODE" (from worker_phase1.md).
    let is_planning = cursor_is_planning(prompt);

    let mut builder = RunnerCommandBuilder::new(bin, work_dir)
        .model(&model)
        .arg("--sandbox")
        .arg(if is_planning { "enabled" } else { "disabled" });

    if is_planning {
        builder = builder.arg("--plan");
    }

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
pub fn run_cursor_resume(
    work_dir: &Path,
    bin: &str,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    // We only receive `message` for resume calls; use the same heuristic.
    let is_planning = cursor_is_planning(message);

    let mut builder = RunnerCommandBuilder::new(bin, work_dir)
        .arg("--resume")
        .arg(session_id)
        .model(&model)
        .arg("--sandbox")
        .arg(if is_planning { "enabled" } else { "disabled" });

    if is_planning {
        builder = builder.arg("--plan");
    }

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
