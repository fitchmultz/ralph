//! Cursor runner plugin implementation.
//!
//! Responsibilities:
//! - Build Cursor CLI commands for run and resume operations.
//! - Parse Cursor JSON response format.
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).

use serde_json::Value as JsonValue;

use crate::commands::run::PhaseType;
use crate::contracts::Runner;
use crate::runner::RunnerError;

use super::super::cli_spec::apply_cursor_options;
use super::super::command::RunnerCommandBuilder;
use super::super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};
use super::apply_analytics_env;

/// Cursor plugin implementation.
pub struct CursorPlugin;

impl RunnerPlugin for CursorPlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Cursor.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir).model(&ctx.model);
        let builder = apply_analytics_env(builder, &Runner::Cursor, &ctx.model);
        let builder = apply_cursor_options(
            builder,
            ctx.runner_cli,
            ctx.phase_type.unwrap_or(PhaseType::Implementation),
        );

        Ok(builder
            .arg("--print")
            .output_format("stream-json")
            .arg(ctx.prompt)
            .build())
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir)
            .arg("--resume")
            .arg(ctx.session_id)
            .model(&ctx.model);
        let builder = apply_analytics_env(builder, &Runner::Cursor, &ctx.model);
        let builder = apply_cursor_options(
            builder,
            ctx.runner_cli,
            ctx.phase_type.unwrap_or(PhaseType::Implementation),
        );

        Ok(builder
            .arg("--print")
            .output_format("stream-json")
            .arg(ctx.message)
            .build())
    }

    fn parse_response_line(&self, line: &str, _buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line).ok()?;
        CursorResponseParser.parse_json(&json)
    }
}

/// Response parser for Cursor's JSON format.
pub struct CursorResponseParser;

impl CursorResponseParser {
    /// Parse Cursor JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue) -> Option<String> {
        // Cursor uses message_end format
        if json.get("type").and_then(|t| t.as_str()) != Some("message_end") {
            return None;
        }

        let message = json.get("message")?;
        if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            return None;
        }

        let content = message.get("content")?;
        super::extract_text_content(content)
    }
}

impl ResponseParser for CursorResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        self.parse_json(json)
    }

    fn runner_id(&self) -> &str {
        "cursor"
    }
}
