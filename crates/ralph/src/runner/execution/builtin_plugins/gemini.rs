//! Gemini runner plugin implementation.
//!
//! Responsibilities:
//! - Build Gemini CLI commands for run and resume operations.
//! - Parse Gemini JSON response format.
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).

use serde_json::Value as JsonValue;

use crate::contracts::Runner;
use crate::runner::RunnerError;

use super::super::cli_spec::apply_gemini_options;
use super::super::command::RunnerCommandBuilder;
use super::super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};
use super::{apply_analytics_env, extract_text_content};

/// Gemini plugin implementation.
pub struct GeminiPlugin;

impl RunnerPlugin for GeminiPlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Gemini.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir);
        let builder = apply_analytics_env(builder, &Runner::Gemini, &ctx.model);
        let builder = apply_gemini_options(builder, ctx.runner_cli);

        Ok(builder
            .model(&ctx.model)
            .output_format("stream-json")
            .stdin_payload(Some(ctx.prompt.as_bytes().to_vec()))
            .build())
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(ctx.bin, ctx.work_dir);
        let builder = apply_analytics_env(builder, &Runner::Gemini, &ctx.model);
        let builder = apply_gemini_options(builder, ctx.runner_cli);

        Ok(builder
            .arg("--resume")
            .arg(ctx.session_id)
            .model(&ctx.model)
            .output_format("stream-json")
            .arg(ctx.message)
            .build())
    }

    fn parse_response_line(&self, line: &str, _buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line)
            .inspect_err(|e| log::trace!("Gemini response not valid JSON: {}", e))
            .ok()?;
        GeminiResponseParser.parse_json(&json)
    }
}

/// Response parser for Gemini's JSON format.
pub struct GeminiResponseParser;

impl GeminiResponseParser {
    /// Parse Gemini JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("message") {
            return None;
        }

        if json.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            return None;
        }

        let content = json.get("content")?;
        extract_text_content(content)
    }
}

impl ResponseParser for GeminiResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        self.parse_json(json)
    }

    fn runner_id(&self) -> &str {
        "gemini"
    }
}
