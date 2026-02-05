//! Execution helpers for plugin-provided runners.
//!
//! Responsibilities:
//! - Build commands for plugin runner executables using a stable protocol.
//! - Pass resolved runner CLI options + plugin config securely (never log raw blobs).
//!
//! Not handled here:
//! - Discovering plugins (see `crate::plugins`).
//! - Validating models beyond basic non-empty checks (plugin may enforce).
//!
//! Protocol (required):
//! - `bin run --model <id> --output-format stream-json [--session <id>]` reads prompt from stdin.
//! - `bin resume --session <id> --model <id> --output-format stream-json <message>` OR stdin message (choose one; below uses arg).
//! - The runner MUST emit newline-delimited JSON objects compatible with Ralph's streaming parser.
//!
//! Env passed:
//! - `RALPH_PLUGIN_ID`
//! - `RALPH_PLUGIN_CONFIG_JSON` (opaque; may be empty)
//! - `RALPH_RUNNER_CLI_JSON` (resolved normalized options)

use std::path::Path;
use std::time::Duration;

use crate::contracts::Model;
use crate::runner::{OutputHandler, OutputStream, RunnerError, RunnerOutput};

use super::cli_options::ResolvedRunnerCliOptions;
use super::command::RunnerCommandBuilder;
use super::process::run_with_streaming_json;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_plugin_runner(
    work_dir: &Path,
    bin: &str,
    plugin_id: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    prompt: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    session_id: Option<&str>,
    plugin_config_json: Option<String>,
) -> Result<RunnerOutput, RunnerError> {
    let runner_cli_json = serde_json::to_string(&runner_cli).unwrap_or_else(|_| "{}".to_string());
    let cfg = plugin_config_json.unwrap_or_else(|| "{}".to_string());

    let mut builder = RunnerCommandBuilder::new(bin, work_dir)
        .env("RALPH_PLUGIN_ID", plugin_id)
        .env("RALPH_PLUGIN_CONFIG_JSON", &cfg)
        .env("RALPH_RUNNER_CLI_JSON", &runner_cli_json);

    builder = builder.arg("run").arg("--model").arg(model.as_str());
    builder = builder.arg("--output-format").arg("stream-json");
    if let Some(id) = session_id {
        builder = builder.arg("--session").arg(id);
    }

    let (cmd, payload, _guards) = builder
        .arg("-")
        .stdin_payload(Some(prompt.as_bytes().to_vec()))
        .build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        crate::contracts::Runner::Plugin(plugin_id.to_string()),
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_plugin_runner_resume(
    work_dir: &Path,
    bin: &str,
    plugin_id: &str,
    runner_cli: ResolvedRunnerCliOptions,
    model: Model,
    session_id: &str,
    message: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    plugin_config_json: Option<String>,
) -> Result<RunnerOutput, RunnerError> {
    let runner_cli_json = serde_json::to_string(&runner_cli).unwrap_or_else(|_| "{}".to_string());
    let cfg = plugin_config_json.unwrap_or_else(|| "{}".to_string());

    let builder = RunnerCommandBuilder::new(bin, work_dir)
        .env("RALPH_PLUGIN_ID", plugin_id)
        .env("RALPH_PLUGIN_CONFIG_JSON", &cfg)
        .env("RALPH_RUNNER_CLI_JSON", &runner_cli_json)
        .arg("resume")
        .arg("--session")
        .arg(session_id)
        .arg("--model")
        .arg(model.as_str())
        .arg("--output-format")
        .arg("stream-json")
        .arg(message);

    let (cmd, payload, _guards) = builder.build();

    run_with_streaming_json(
        cmd,
        payload.as_deref(),
        crate::contracts::Runner::Plugin(plugin_id.to_string()),
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}
