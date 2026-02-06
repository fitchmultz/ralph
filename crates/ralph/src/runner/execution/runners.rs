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

use super::super::{Model, OutputHandler, OutputStream, RunnerError, RunnerOutput};
use super::cli_options::ResolvedRunnerCliOptions;
use super::cli_spec;
use super::command::RunnerCommandBuilder;
use super::process::run_with_streaming_json;
use crate::commands::run::PhaseType;
use crate::constants::paths::{ENV_MODEL_USED, ENV_RUNNER_USED};
use crate::contracts::Runner;

/// Apply analytics environment variables to track which runner/model was actually used.
fn apply_analytics_env(
    builder: RunnerCommandBuilder,
    runner: &Runner,
    model: &Model,
) -> RunnerCommandBuilder {
    builder
        .env(ENV_RUNNER_USED, runner.id())
        .env(ENV_MODEL_USED, model.as_str())
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
    let builder = apply_analytics_env(builder, &Runner::Cursor, &model);
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
    let builder = apply_analytics_env(builder, &Runner::Cursor, &model);
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
