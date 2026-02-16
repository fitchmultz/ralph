//! Codex runner plugin implementation.
//!
//! Responsibilities:
//! - Build Codex CLI commands for run and resume operations.
//! - Parse Codex JSON response format.
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).

use serde_json::Value as JsonValue;

use crate::contracts::Runner;
use crate::runner::RunnerError;

use super::super::cli_spec::apply_codex_global_options;
use super::super::command::RunnerCommandBuilder;
use super::super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};
use super::apply_analytics_env;

/// Codex plugin implementation.
pub struct CodexPlugin;

impl RunnerPlugin for CodexPlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Codex.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir);
        let builder = apply_analytics_env(builder, &Runner::Codex, &ctx.model);
        let builder = apply_codex_global_options(builder, ctx.runner_cli);

        Ok(builder
            .arg("exec")
            .legacy_json_format()
            .model(&ctx.model)
            .reasoning_effort(ctx.reasoning_effort)
            .arg("-")
            .stdin_payload(Some(ctx.prompt.as_bytes().to_vec()))
            .build())
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir);
        let builder = apply_analytics_env(builder, &Runner::Codex, &ctx.model);
        let builder = apply_codex_global_options(builder, ctx.runner_cli);

        Ok(builder
            .arg("exec")
            .arg("resume")
            .arg(ctx.session_id)
            .legacy_json_format()
            .model(&ctx.model)
            .reasoning_effort(ctx.reasoning_effort)
            .arg(ctx.message)
            .build())
    }

    fn parse_response_line(&self, line: &str, _buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line)
            .inspect_err(|e| log::trace!("Codex response not valid JSON: {}", e))
            .ok()?;
        CodexResponseParser.parse_json(&json)
    }
}

/// Response parser for Codex's JSON format.
pub struct CodexResponseParser;

impl CodexResponseParser {
    /// Parse Codex JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("item.completed") {
            return None;
        }

        let item = json.get("item")?;
        if item.get("type").and_then(|t| t.as_str()) != Some("agent_message") {
            return None;
        }

        let text = item.get("text").and_then(|t| t.as_str())?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

impl ResponseParser for CodexResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        self.parse_json(json)
    }

    fn runner_id(&self) -> &str {
        "codex"
    }
}
