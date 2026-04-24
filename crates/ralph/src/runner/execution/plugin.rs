//! Execution helpers for plugin-provided runners.
//!
//! Purpose:
//! - Execution helpers for plugin-provided runners.
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
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::path::Path;
use std::time::Duration;

use crate::constants::paths::{ENV_MODEL_USED, ENV_RUNNER_USED};
use crate::contracts::Model;
use crate::contracts::Runner;
use crate::runner::error::runner_execution_error_with_source;
use crate::runner::{OutputHandler, OutputStream, RunnerError, RunnerOutput};

use super::cli_options::ResolvedRunnerCliOptions;
use super::command::RunnerCommandBuilder;
use super::process::run_with_streaming_json;

/// Serialize a value to JSON for plugin environment variables.
///
/// On failure, returns a `RunnerError` with context that includes the plugin_id
/// and indicates which JSON blob failed (e.g., "plugin config" or "runner cli").
/// This ensures serialization errors are propagated rather than silently falling
/// back to "{}", which could cause plugins to run with incorrect configuration.
pub(crate) fn serialize_plugin_env_json<T: serde::Serialize>(
    plugin_id: &str,
    bin: &str,
    what: &'static str,
    value: &T,
) -> Result<String, RunnerError> {
    match serde_json::to_string(value) {
        Ok(json) => Ok(json),
        Err(err) => {
            let step = format!("serialize {what} JSON");
            Err(runner_execution_error_with_source(
                &Runner::Plugin(plugin_id.to_string()),
                bin,
                &step,
                err,
            ))
        }
    }
}

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
    let runner_cli_json = serialize_plugin_env_json(plugin_id, bin, "runner cli", &runner_cli)?;
    let cfg = plugin_config_json.unwrap_or_else(|| "{}".to_string());

    let mut builder = RunnerCommandBuilder::new(bin, work_dir)
        .env("RALPH_PLUGIN_ID", plugin_id)
        .env("RALPH_PLUGIN_CONFIG_JSON", &cfg)
        .env("RALPH_RUNNER_CLI_JSON", &runner_cli_json)
        .env(ENV_RUNNER_USED, plugin_id)
        .env(ENV_MODEL_USED, model.as_str());

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
        Runner::Plugin(plugin_id.to_string()),
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
    let runner_cli_json = serialize_plugin_env_json(plugin_id, bin, "runner cli", &runner_cli)?;
    let cfg = plugin_config_json.unwrap_or_else(|| "{}".to_string());

    let builder = RunnerCommandBuilder::new(bin, work_dir)
        .env("RALPH_PLUGIN_ID", plugin_id)
        .env("RALPH_PLUGIN_CONFIG_JSON", &cfg)
        .env("RALPH_RUNNER_CLI_JSON", &runner_cli_json)
        .env(ENV_RUNNER_USED, plugin_id)
        .env(ENV_MODEL_USED, model.as_str())
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
        Runner::Plugin(plugin_id.to_string()),
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

#[cfg(test)]
mod tests {
    use super::serialize_plugin_env_json;
    use serde::{Serialize, Serializer};

    // A struct that always fails serialization (used to test error propagation)
    struct AlwaysFailsSerialize;

    impl Serialize for AlwaysFailsSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(serde::ser::Error::custom("intentional test failure"))
        }
    }

    #[test]
    fn serialize_plugin_config_failure_includes_plugin_id_and_context() {
        let err = serialize_plugin_env_json(
            "test.plugin",
            "dummy-bin",
            "plugin config",
            &AlwaysFailsSerialize,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("test.plugin"),
            "error should include plugin_id: {}",
            msg
        );
        assert!(
            msg.contains("serialize plugin config JSON"),
            "error should indicate what failed: {}",
            msg
        );
    }

    #[test]
    fn serialize_runner_cli_failure_includes_plugin_id_and_context() {
        let err = serialize_plugin_env_json(
            "my.plugin",
            "/bin/my-runner",
            "runner cli",
            &AlwaysFailsSerialize,
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("my.plugin"),
            "error should include plugin_id: {}",
            msg
        );
        assert!(
            msg.contains("serialize runner cli JSON"),
            "error should indicate what failed: {}",
            msg
        );
    }

    #[derive(Serialize)]
    struct GoodConfig {
        name: String,
        enabled: bool,
    }

    #[test]
    fn serialize_plugin_config_success_returns_valid_json() {
        let result = serialize_plugin_env_json(
            "test.plugin",
            "dummy-bin",
            "plugin config",
            &GoodConfig {
                name: "test".to_string(),
                enabled: true,
            },
        );

        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("enabled"));
    }
}
