//! Pi runner plugin implementation.
//!
//! Responsibilities:
//! - Build Pi CLI commands for run and resume operations.
//! - Parse Pi JSON response format.
//! - Resolve Pi session paths for resume operations.
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).

use std::path::Path;

use serde_json::Value as JsonValue;

use crate::contracts::Runner;
use crate::runner::{
    ResolvedRunnerCliOptions, RunnerError, runner_execution_error,
    runner_execution_error_with_source,
};

use super::super::cli_spec::apply_pi_options;
use super::super::command::RunnerCommandBuilder;
use super::super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};
use super::{apply_analytics_env, extract_text_content};

/// Pi plugin implementation.
pub struct PiPlugin;

impl RunnerPlugin for PiPlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Pi.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        self.build_pi_command(
            ctx.work_dir,
            ctx.bin,
            ctx.runner_cli,
            &ctx.model,
            ctx.prompt,
            None,
        )
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let session_path = resolve_pi_session_path(ctx.work_dir, ctx.session_id)?;
        self.build_pi_command(
            ctx.work_dir,
            ctx.bin,
            ctx.runner_cli,
            &ctx.model,
            ctx.message,
            Some(&session_path),
        )
    }

    fn parse_response_line(&self, line: &str, _buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line).ok()?;
        PiResponseParser.parse_json(&json)
    }
}

impl PiPlugin {
    fn build_pi_command(
        &self,
        work_dir: &Path,
        bin: &str,
        runner_cli: ResolvedRunnerCliOptions,
        model: &crate::contracts::Model,
        prompt: &str,
        session_path: Option<&Path>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let builder = RunnerCommandBuilder::new(bin, work_dir);
        let builder = apply_analytics_env(builder, &Runner::Pi, model);
        let builder = apply_pi_options(builder, runner_cli);

        let builder = if let Some(path) = session_path {
            builder
                .arg("--session")
                .arg(path.to_string_lossy().as_ref())
        } else {
            builder
        };

        Ok(builder
            .model(model)
            .arg("--mode")
            .arg("json")
            .arg(prompt)
            .build())
    }
}

/// Resolve the path to a Pi session file for resume operations.
pub(crate) fn resolve_pi_session_path(
    work_dir: &Path,
    session_id: &str,
) -> Result<std::path::PathBuf, RunnerError> {
    let direct = std::path::Path::new(session_id);
    if direct.is_file() {
        return Ok(direct.to_path_buf());
    }

    let base = pi_agent_root().ok_or_else(|| {
        runner_execution_error(
            &Runner::Pi,
            "pi",
            "resolve PI_CODING_AGENT_DIR or HOME for session lookup",
        )
    })?;
    let sessions_dir = base.join("sessions");
    let workspace_dir = sessions_dir.join(pi_session_dir_name(work_dir));
    let suffix = format!("_{session_id}.jsonl");

    let entries = std::fs::read_dir(&workspace_dir).map_err(|err| {
        runner_execution_error_with_source(
            &Runner::Pi,
            "pi",
            &format!("read pi session dir {}", workspace_dir.display()),
            err,
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|err| {
            runner_execution_error_with_source(&Runner::Pi, "pi", "read pi session entry", err)
        })?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(&suffix))
            .unwrap_or(false)
        {
            return Ok(path);
        }
    }

    Err(runner_execution_error(
        &Runner::Pi,
        "pi",
        &format!("pi session file not found for id {session_id}"),
    ))
}

/// Get the Pi agent root directory from environment or home directory.
pub(crate) fn pi_agent_root() -> Option<std::path::PathBuf> {
    if let Some(value) = std::env::var_os("PI_CODING_AGENT_DIR") {
        return Some(std::path::PathBuf::from(value));
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".pi").join("agent"))
}

/// Generate the session directory name for a given workspace path.
pub fn pi_session_dir_name(work_dir: &Path) -> String {
    let mut path = work_dir.to_string_lossy().to_string();
    if let Some(stripped) = path.strip_prefix("\\\\?\\") {
        path = stripped.to_string();
    }
    let trimmed = path.trim_start_matches(['/', '\\']);
    let normalized = trimmed.replace(['/', '\\'], "-");
    format!("--{}--", normalized)
}

/// Response parser for Pi's JSON format.
pub struct PiResponseParser;

#[allow(dead_code)]
impl PiResponseParser {
    /// Parse Pi JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue) -> Option<String> {
        // Pi uses a generic JSON format with type="result"
        if json.get("type").and_then(|t| t.as_str()) != Some("result") {
            return None;
        }

        let result = json.get("result")?;
        extract_text_content(result)
    }
}

impl ResponseParser for PiResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        self.parse_json(json)
    }

    fn runner_id(&self) -> &str {
        "pi"
    }
}
