//! Claude runner plugin implementation.
//!
//! Responsibilities:
//! - Build Claude CLI commands for run and resume operations.
//! - Parse Claude JSON response format.
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).

use serde_json::Value as JsonValue;

use crate::contracts::Runner;
use crate::runner::RunnerError;

use super::super::cli_spec::apply_claude_options;
use super::super::command::RunnerCommandBuilder;
use super::super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};
use super::apply_analytics_env;

/// Claude plugin implementation.
pub struct ClaudePlugin;

impl RunnerPlugin for ClaudePlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Claude.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir);
        let builder = apply_analytics_env(builder, &Runner::Claude, &ctx.model);
        let builder = apply_claude_options(builder, ctx.runner_cli);

        Ok(builder
            .arg("--verbose")
            .arg("-p")
            .model(&ctx.model)
            .permission_mode(ctx.permission_mode)
            .output_format("stream-json")
            .stdin_payload(Some(ctx.prompt.as_bytes().to_vec()))
            .build())
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir);
        let builder = apply_analytics_env(builder, &Runner::Claude, &ctx.model);
        let builder = apply_claude_options(builder, ctx.runner_cli);

        Ok(builder
            .arg("--resume")
            .arg(ctx.session_id)
            .arg("--verbose")
            .model(&ctx.model)
            .permission_mode(ctx.permission_mode)
            .output_format("stream-json")
            .arg("-p")
            .arg(ctx.message)
            .build())
    }

    fn parse_response_line(&self, line: &str, _buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line)
            .inspect_err(|e| log::trace!("Claude response not valid JSON: {}", e))
            .ok()?;
        ClaudeResponseParser.parse_json(&json)
    }
}

/// Response parser for Claude's JSON format.
pub struct ClaudeResponseParser;

impl ClaudeResponseParser {
    /// Parse Claude JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            return None;
        }

        let message = json.get("message")?;
        let content = message.get("content")?.as_array()?;

        let mut parts = Vec::new();
        for item in content {
            if item.get("type").and_then(|t| t.as_str()) != Some("text") {
                continue;
            }
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }
}

impl ResponseParser for ClaudeResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        self.parse_json(json)
    }

    fn runner_id(&self) -> &str {
        "claude"
    }
}
