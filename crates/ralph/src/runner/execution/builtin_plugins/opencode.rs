//! Opencode runner plugin implementation.
//!
//! Responsibilities:
//! - Build Opencode CLI commands for run and resume operations.
//! - Parse Opencode JSON response format.
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).

use serde_json::Value as JsonValue;

use crate::contracts::Runner;
use crate::runner::{RunnerError, runner_execution_error_with_source};

use super::super::command::RunnerCommandBuilder;
use super::super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};
use super::apply_analytics_env;

/// Opencode plugin implementation.
pub struct OpencodePlugin;

impl RunnerPlugin for OpencodePlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Opencode.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        let builder = apply_analytics_env(
            RunnerCommandBuilder::new(ctx.bin, ctx.work_dir),
            &Runner::Opencode,
            &ctx.model,
        );

        Ok(builder
            .arg("run")
            .model(&ctx.model)
            .opencode_format()
            .with_temp_prompt_file(ctx.prompt)
            .map_err(|err| {
                runner_execution_error_with_source(
                    &Runner::Opencode,
                    ctx.bin,
                    "create temp prompt file",
                    err,
                )
            })?
            .build())
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let builder = apply_analytics_env(
            RunnerCommandBuilder::new(ctx.bin, ctx.work_dir),
            &Runner::Opencode,
            &ctx.model,
        );

        Ok(builder
            .arg("run")
            .arg("-s")
            .arg(ctx.session_id)
            .model(&ctx.model)
            .opencode_format()
            .arg("--")
            .arg(ctx.message)
            .build())
    }

    fn parse_response_line(&self, line: &str, buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line)
            .inspect_err(|e| log::trace!("Opencode response not valid JSON: {}", e))
            .ok()?;
        OpencodeResponseParser.parse_json(&json, buffer)
    }
}

/// Response parser for Opencode's JSON format.
pub struct OpencodeResponseParser;

impl OpencodeResponseParser {
    /// Parse Opencode JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue, buffer: &mut String) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("text") {
            return None;
        }

        let text = json
            .get("part")
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())?;

        if text.is_empty() {
            return None;
        }

        buffer.push_str(text);
        Some(buffer.clone())
    }
}

impl ResponseParser for OpencodeResponseParser {
    fn parse(&self, json: &JsonValue, buffer: &mut String) -> Option<String> {
        self.parse_json(json, buffer)
    }

    fn runner_id(&self) -> &str {
        "opencode"
    }
}
