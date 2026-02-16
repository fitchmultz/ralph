//! Kimi runner plugin implementation.
//!
//! Responsibilities:
//! - Build Kimi CLI commands for run and resume operations.
//! - Parse Kimi JSON response format.
//! - Manage session ID handling (Kimi requires managed session IDs).
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).

use serde_json::Value as JsonValue;

use crate::contracts::Runner;
use crate::runner::RunnerError;

use super::super::cli_spec::apply_kimi_options;
use super::super::command::RunnerCommandBuilder;
use super::super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};
use super::apply_analytics_env;

/// Kimi plugin implementation.
pub struct KimiPlugin;

impl RunnerPlugin for KimiPlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Kimi.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir);
        let builder = apply_analytics_env(builder, &Runner::Kimi, &ctx.model);
        let builder = apply_kimi_options(builder, ctx.runner_cli);

        let builder = if let Some(ref session_id) = ctx.session_id {
            builder.arg("--session").arg(session_id)
        } else {
            builder
        };

        Ok(builder
            .model(&ctx.model)
            .arg("--print")
            .arg("--prompt")
            .arg(ctx.prompt)
            .output_format("stream-json")
            .build())
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        // Kimi reuses the same command structure for resume, just with a session_id
        let run_ctx = RunContext {
            work_dir: ctx.work_dir,
            bin: ctx.bin,
            model: ctx.model,
            prompt: ctx.message,
            timeout: ctx.timeout,
            output_handler: ctx.output_handler,
            output_stream: ctx.output_stream,
            runner_cli: ctx.runner_cli,
            reasoning_effort: ctx.reasoning_effort,
            permission_mode: ctx.permission_mode,
            phase_type: ctx.phase_type,
            session_id: Some(ctx.session_id.to_string()),
        };

        self.build_run_command(run_ctx)
    }

    fn parse_response_line(&self, line: &str, _buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line)
            .inspect_err(|e| log::trace!("Kimi response not valid JSON: {}", e))
            .ok()?;
        KimiResponseParser.parse_json(&json)
    }

    fn requires_managed_session_id(&self) -> bool {
        true
    }
}

/// Response parser for Kimi's JSON format.
pub struct KimiResponseParser;

impl KimiResponseParser {
    /// Parse Kimi JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue) -> Option<String> {
        // Kimi format has role="assistant" at top level with content array
        if json.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            return None;
        }

        let content = json.get("content")?.as_array()?;
        let mut text_parts = Vec::new();

        for item in content {
            if item.get("type").and_then(|t| t.as_str()) != Some("text") {
                continue;
            }
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    text_parts.push(trimmed.to_string());
                }
            }
        }

        if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        }
    }
}

impl ResponseParser for KimiResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        self.parse_json(json)
    }

    fn runner_id(&self) -> &str {
        "kimi"
    }
}
