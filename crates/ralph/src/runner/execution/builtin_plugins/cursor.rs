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

fn assistant_stream_chunk(content: &JsonValue) -> Option<String> {
    match content {
        JsonValue::String(text) => {
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }
        JsonValue::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    out.push_str(text);
                }
            }
            if out.is_empty() { None } else { Some(out) }
        }
        _ => None,
    }
}

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

    fn parse_response_line(&self, line: &str, buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line)
            .inspect_err(|e| log::trace!("Cursor response not valid JSON: {}", e))
            .ok()?;
        CursorResponseParser.parse_json(&json, buffer)
    }
}

/// Response parser for Cursor's JSON format.
pub struct CursorResponseParser;

impl CursorResponseParser {
    /// Parse Cursor JSON response format.
    ///
    /// Current Cursor Agent `stream-json` emits `assistant` events (and optional
    /// `--stream-partial-output` text deltas), optional legacy `message_end` envelopes,
    /// and a terminal `result` event with the full concatenated assistant text.
    pub(crate) fn parse_json(&self, json: &JsonValue, buffer: &mut String) -> Option<String> {
        match json.get("type").and_then(|t| t.as_str()) {
            Some("assistant") => {
                let message = json.get("message")?;
                if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                    return None;
                }

                let content = message.get("content")?;
                let chunk = assistant_stream_chunk(content)?;
                buffer.push_str(&chunk);
                Some(buffer.clone())
            }
            // Legacy/alternate envelope used by some Cursor Agent builds.
            Some("message_end") => {
                let message = json.get("message")?;
                if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                    return None;
                }

                let content = message.get("content")?;
                let text = super::extract_text_content(content)?;
                buffer.clear();
                buffer.push_str(&text);
                Some(buffer.clone())
            }
            Some("result") => {
                let result = json.get("result")?;
                let text = super::extract_text_content(result)?;
                buffer.clear();
                buffer.push_str(&text);
                Some(buffer.clone())
            }
            _ => None,
        }
    }
}

impl ResponseParser for CursorResponseParser {
    fn parse(&self, json: &JsonValue, buffer: &mut String) -> Option<String> {
        self.parse_json(json, buffer)
    }

    fn runner_id(&self) -> &str {
        "cursor"
    }
}
