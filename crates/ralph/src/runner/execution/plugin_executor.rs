//! Plugin-based runner execution orchestration.
//!
//! Purpose:
//! - Plugin-based runner execution orchestration.
//!
//! Responsibilities:
//! - Dispatch execution to RunnerPlugin trait implementations.
//! - Unify built-in and external plugin execution paths.
//! - Provide centralized response parsing through ResponseParserRegistry.
//!
//! Not handled here:
//! - Concrete runner logic (handled by RunnerPlugin implementations).
//! - Response parsing details (handled by ResponseParser implementations).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - PluginExecutor must be created once and reused for consistent behavior.
//! - All built-in runners are pre-registered at construction time.
//!
//! Note: Module-level dead_code allow is required because this module provides
//! orchestrator types and registry structures that may not use all fields in
//! every execution path, but are part of the public API for extension.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;

use std::time::Duration;

use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, Model, Runner};
use crate::plugins::registry::PluginRegistry;
use crate::runner::{
    OutputHandler, OutputStream, ResolvedRunnerCliOptions, RunnerError, RunnerOutput,
};

#[cfg(test)]
mod tests;

use super::builtin_plugins::BuiltInRunnerPlugin;
use super::plugin_trait::{ResumeContext, RunContext, RunnerMetadata, RunnerPlugin};
use super::process::run_with_streaming_json;
use super::response::ResponseParserRegistry;

/// Executor that dispatches to RunnerPlugin implementations.
pub struct PluginExecutor {
    /// Cache of built-in runner plugins
    built_ins: HashMap<Runner, BuiltInRunnerPlugin>,
    /// Registry for parsing responses
    response_parsers: ResponseParserRegistry,
}

impl Default for PluginExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginExecutor {
    /// Create a new PluginExecutor with all built-in runners registered.
    pub fn new() -> Self {
        let mut built_ins = HashMap::new();

        // Initialize all built-in runners
        built_ins.insert(Runner::Codex, BuiltInRunnerPlugin::Codex);
        built_ins.insert(Runner::Opencode, BuiltInRunnerPlugin::Opencode);
        built_ins.insert(Runner::Gemini, BuiltInRunnerPlugin::Gemini);
        built_ins.insert(Runner::Claude, BuiltInRunnerPlugin::Claude);
        built_ins.insert(Runner::Kimi, BuiltInRunnerPlugin::Kimi);
        built_ins.insert(Runner::Pi, BuiltInRunnerPlugin::Pi);
        built_ins.insert(Runner::Cursor, BuiltInRunnerPlugin::Cursor);

        Self {
            built_ins,
            response_parsers: ResponseParserRegistry::new(),
        }
    }

    /// Get metadata for a runner.
    pub fn metadata(&self, runner: &Runner) -> RunnerMetadata {
        match runner {
            Runner::Plugin(plugin_id) => RunnerMetadata {
                id: plugin_id.clone(),
                name: format!("Plugin: {}", plugin_id),
                supports_resume: true, // External plugins assume resume support
                default_model: None,
            },
            _ => self
                .built_ins
                .get(runner)
                .map(|p| p.metadata())
                .unwrap_or_else(|| RunnerMetadata {
                    id: runner.id().to_string(),
                    name: runner.id().to_string(),
                    supports_resume: false,
                    default_model: None,
                }),
        }
    }

    /// Execute a prompt using the appropriate plugin.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        runner: Runner,
        work_dir: &Path,
        bin: &str,
        model: Model,
        reasoning_effort: Option<crate::contracts::ReasoningEffort>,
        runner_cli: ResolvedRunnerCliOptions,
        prompt: &str,
        timeout: Option<Duration>,
        permission_mode: Option<ClaudePermissionMode>,
        output_handler: Option<OutputHandler>,
        output_stream: OutputStream,
        phase_type: PhaseType,
        session_id: Option<String>,
        plugins: Option<&PluginRegistry>,
    ) -> Result<RunnerOutput, RunnerError> {
        match &runner {
            Runner::Plugin(plugin_id) => self.run_external_plugin(
                plugin_id,
                work_dir,
                bin,
                runner_cli,
                model,
                prompt,
                timeout,
                output_handler.clone(),
                output_stream,
                session_id,
                plugins,
            ),
            _ => {
                let plugin = self.built_ins.get(&runner).ok_or_else(|| {
                    RunnerError::Other(anyhow::anyhow!(
                        "No plugin implementation for runner: {}",
                        runner.id()
                    ))
                })?;

                let ctx = RunContext {
                    work_dir,
                    bin,
                    model,
                    prompt,
                    timeout,
                    output_handler: output_handler.clone(),
                    output_stream,
                    runner_cli,
                    reasoning_effort,
                    permission_mode,
                    phase_type: Some(phase_type),
                    session_id: session_id.clone(),
                };

                let (cmd, payload, _guards) = plugin.build_run_command(ctx)?;

                let mut output = run_with_streaming_json(
                    cmd,
                    payload.as_deref(),
                    runner.clone(),
                    bin,
                    timeout,
                    output_handler.clone(),
                    output_stream,
                )?;

                // For runners that require Ralph-managed session IDs (like kimi),
                // preserve the session_id since it won't be in the runner's output
                if self.requires_managed_session_id(&runner) {
                    output.session_id = session_id;
                }

                Ok(output)
            }
        }
    }

    /// Resume a session using the appropriate plugin.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    pub fn resume(
        &self,
        runner: Runner,
        work_dir: &Path,
        bin: &str,
        model: Model,
        reasoning_effort: Option<crate::contracts::ReasoningEffort>,
        runner_cli: ResolvedRunnerCliOptions,
        session_id: &str,
        message: &str,
        timeout: Option<Duration>,
        permission_mode: Option<ClaudePermissionMode>,
        output_handler: Option<OutputHandler>,
        output_stream: OutputStream,
        phase_type: PhaseType,
        plugins: Option<&PluginRegistry>,
    ) -> Result<RunnerOutput, RunnerError> {
        match &runner {
            Runner::Plugin(plugin_id) => self.resume_external_plugin(
                plugin_id,
                work_dir,
                bin,
                runner_cli,
                model,
                session_id,
                message,
                timeout,
                output_handler.clone(),
                output_stream,
                plugins,
            ),
            _ => {
                let plugin = self.built_ins.get(&runner).ok_or_else(|| {
                    RunnerError::Other(anyhow::anyhow!(
                        "No plugin implementation for runner: {}",
                        runner.id()
                    ))
                })?;

                let ctx = ResumeContext {
                    work_dir,
                    bin,
                    model,
                    session_id,
                    message,
                    timeout,
                    output_handler: output_handler.clone(),
                    output_stream,
                    runner_cli,
                    reasoning_effort,
                    permission_mode,
                    phase_type: Some(phase_type),
                };

                let (cmd, payload, _guards) = plugin.build_resume_command(ctx)?;

                run_with_streaming_json(
                    cmd,
                    payload.as_deref(),
                    runner,
                    bin,
                    timeout,
                    output_handler.clone(),
                    output_stream,
                )
            }
        }
    }

    /// Check if a runner requires Ralph-managed session IDs.
    pub fn requires_managed_session_id(&self, runner: &Runner) -> bool {
        match runner {
            Runner::Plugin(_) => false, // External plugins manage their own
            _ => self
                .built_ins
                .get(runner)
                .map(|p| p.requires_managed_session_id())
                .unwrap_or(false),
        }
    }

    /// Extract the final assistant response from runner output.
    pub fn extract_final_response(&self, runner: &Runner, stdout: &str) -> Option<String> {
        self.response_parsers.extract_final_response(runner, stdout)
    }

    /// Execute an external plugin runner.
    #[allow(clippy::too_many_arguments)]
    fn run_external_plugin(
        &self,
        plugin_id: &str,
        work_dir: &Path,
        bin: &str,
        runner_cli: ResolvedRunnerCliOptions,
        model: Model,
        prompt: &str,
        timeout: Option<Duration>,
        output_handler: Option<OutputHandler>,
        output_stream: OutputStream,
        session_id: Option<String>,
        plugins: Option<&PluginRegistry>,
    ) -> Result<RunnerOutput, RunnerError> {
        // Get plugin config if available
        let plugin_config_json = plugins
            .and_then(|p| p.plugin_config_blob(plugin_id))
            .map(|v| super::serialize_plugin_env_json(plugin_id, bin, "plugin config", &v))
            .transpose()?;

        super::plugin::run_plugin_runner(
            work_dir,
            bin,
            plugin_id,
            runner_cli,
            model,
            prompt,
            timeout,
            output_handler,
            output_stream,
            session_id.as_deref(),
            plugin_config_json,
        )
    }

    /// Resume an external plugin runner.
    #[allow(clippy::too_many_arguments)]
    fn resume_external_plugin(
        &self,
        plugin_id: &str,
        work_dir: &Path,
        bin: &str,
        runner_cli: ResolvedRunnerCliOptions,
        model: Model,
        session_id: &str,
        message: &str,
        timeout: Option<Duration>,
        output_handler: Option<OutputHandler>,
        output_stream: OutputStream,
        plugins: Option<&PluginRegistry>,
    ) -> Result<RunnerOutput, RunnerError> {
        // Get plugin config if available
        let plugin_config_json = plugins
            .and_then(|p| p.plugin_config_blob(plugin_id))
            .map(|v| super::serialize_plugin_env_json(plugin_id, bin, "plugin config", &v))
            .transpose()?;

        super::plugin::run_plugin_runner_resume(
            work_dir,
            bin,
            plugin_id,
            runner_cli,
            model,
            session_id,
            message,
            timeout,
            output_handler,
            output_stream,
            plugin_config_json,
        )
    }
}
