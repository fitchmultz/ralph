//! External plugin runner dispatch helpers.
//!
//! Purpose:
//! - External plugin runner dispatch helpers.
//!
//! Responsibilities:
//! - Resolve plugin runner enablement, binary paths, and config blobs.
//! - Execute plugin prompt/resume operations through the shared plugin protocol.
//!
//! Non-scope:
//! - Built-in runner execution (see `invoke.rs`).
//! - Plugin manifest discovery (see `crate::plugins`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Plugin IDs are already parsed from `Runner::Plugin`.

use crate::contracts::Model;
use crate::plugins::registry::PluginRegistry;
use crate::runner::{OutputHandler, OutputStream, RunnerError, RunnerOutput, execution};
use anyhow::{Result, anyhow};
use std::path::Path;
use std::time::Duration;

use super::invoke::RunnerInvocation;

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_plugin_operation(
    plugin_id: &str,
    work_dir: &Path,
    runner_cli: execution::ResolvedRunnerCliOptions,
    model: Model,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    invocation: RunnerInvocation<'_>,
    plugins: Option<&PluginRegistry>,
) -> Result<RunnerOutput, RunnerError> {
    let registry = plugins.ok_or_else(|| {
        RunnerError::Other(anyhow!(
            "Plugin registry unavailable for plugin runner: {}",
            plugin_id
        ))
    })?;

    if !registry.is_enabled(plugin_id) {
        return Err(RunnerError::Other(anyhow!(
            "Plugin runner is disabled: {}. Enable it under config.plugins.plugins.{}.enabled=true",
            plugin_id,
            plugin_id
        )));
    }

    let bin_path = registry
        .resolve_runner_bin(plugin_id)
        .map_err(RunnerError::Other)?;
    let bin = bin_path.to_string_lossy().to_string();
    let plugin_cfg = registry
        .plugin_config_blob(plugin_id)
        .map(|v| execution::serialize_plugin_env_json(plugin_id, &bin, "plugin config", &v))
        .transpose()?;

    match invocation {
        RunnerInvocation::Prompt { prompt, session_id } => execution::run_plugin_runner(
            work_dir,
            &bin,
            plugin_id,
            runner_cli,
            model,
            prompt,
            timeout,
            output_handler,
            output_stream,
            session_id.as_deref(),
            plugin_cfg,
        ),
        RunnerInvocation::Resume {
            session_id,
            message,
        } => execution::run_plugin_runner_resume(
            work_dir,
            &bin,
            plugin_id,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
            plugin_cfg,
        ),
    }
}
