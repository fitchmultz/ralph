//! Pi runner plugin implementation.
//!
//! Purpose:
//! - Pi runner plugin implementation.
//!
//! Responsibilities:
//! - Build Pi CLI commands for run and resume operations.
//! - Wrap Pi's Node entrypoint so its process-title mutation cannot expose inherited secrets.
//! - Parse Pi JSON response format.
//! - Resolve Pi session paths for resume operations.
//!
//! Not handled here:
//! - Process execution (handled by parent module).
//! - CLI option resolution (handled by cli_spec module).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::contracts::Runner;
use crate::fsutil;
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

struct PiCommandRequest<'a> {
    work_dir: &'a Path,
    bin: &'a str,
    runner_cli: ResolvedRunnerCliOptions,
    model: &'a crate::contracts::Model,
    prompt: &'a str,
    session_path: Option<&'a Path>,
    reasoning_effort: Option<crate::contracts::ReasoningEffort>,
}

impl RunnerPlugin for PiPlugin {
    fn metadata(&self) -> RunnerMetadata {
        super::BuiltInRunnerPlugin::Pi.metadata()
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        self.build_pi_command(PiCommandRequest {
            work_dir: ctx.work_dir,
            bin: ctx.bin,
            runner_cli: ctx.runner_cli,
            model: &ctx.model,
            prompt: ctx.prompt,
            session_path: None,
            reasoning_effort: ctx.reasoning_effort,
        })
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let session_path = resolve_pi_session_path(ctx.work_dir, ctx.session_id)?;
        self.build_pi_command(PiCommandRequest {
            work_dir: ctx.work_dir,
            bin: ctx.bin,
            runner_cli: ctx.runner_cli,
            model: &ctx.model,
            prompt: ctx.message,
            session_path: Some(&session_path),
            reasoning_effort: ctx.reasoning_effort,
        })
    }

    fn parse_response_line(&self, line: &str, _buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line)
            .inspect_err(|e| log::trace!("Pi response not valid JSON: {}", e))
            .ok()?;
        PiResponseParser.parse_json(&json)
    }
}

impl PiPlugin {
    fn build_pi_command(
        &self,
        request: PiCommandRequest<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        let (builder, mut temp_resources) =
            if let Some(entrypoint) = pi_node_entrypoint(request.bin) {
                pi_wrapper_builder(request.work_dir, request.bin, &entrypoint)?
            } else {
                (
                    RunnerCommandBuilder::new(request.bin, request.work_dir),
                    Vec::new(),
                )
            };
        let builder = apply_analytics_env(builder, &Runner::Pi, request.model);
        let builder = apply_pi_options(builder, request.runner_cli);

        let builder = if let Some(path) = request.session_path {
            builder
                .arg("--session")
                .arg(path.to_string_lossy().as_ref())
        } else {
            builder
        };

        let (cmd, payload, mut builder_resources) = builder
            .model(request.model)
            .thinking_level(request.reasoning_effort)
            .arg("--mode")
            .arg("json")
            .arg(request.prompt)
            .build();
        temp_resources.append(&mut builder_resources);
        Ok((cmd, payload, temp_resources))
    }
}

fn pi_wrapper_builder(
    work_dir: &Path,
    bin: &str,
    entrypoint: &Path,
) -> Result<
    (
        RunnerCommandBuilder,
        Vec<Box<dyn std::any::Any + Send + Sync>>,
    ),
    RunnerError,
> {
    let temp_dir = fsutil::create_ralph_temp_dir("pi-wrapper").map_err(|err| {
        runner_execution_error_with_source(&Runner::Pi, bin, "create pi wrapper temp dir", err)
    })?;
    let mut wrapper = tempfile::Builder::new()
        .prefix("ralph_pi_wrapper_")
        .suffix(".mjs")
        .tempfile_in(temp_dir.path())
        .map_err(|err| {
            runner_execution_error_with_source(&Runner::Pi, bin, "create pi wrapper", err)
        })?;

    wrapper
        .write_all(PI_WRAPPER_SOURCE.as_bytes())
        .map_err(|err| {
            runner_execution_error_with_source(&Runner::Pi, bin, "write pi wrapper", err)
        })?;
    wrapper.flush().map_err(|err| {
        runner_execution_error_with_source(&Runner::Pi, bin, "flush pi wrapper", err)
    })?;

    let wrapper_path = wrapper.path().to_string_lossy().to_string();
    let entrypoint_path = entrypoint.to_string_lossy().to_string();
    let builder = RunnerCommandBuilder::new("node", work_dir)
        .arg(&wrapper_path)
        .env("RALPH_PI_BIN", bin)
        .env("RALPH_PI_ENTRYPOINT", &entrypoint_path);
    Ok((builder, vec![Box::new(wrapper), Box::new(temp_dir)]))
}

fn pi_node_entrypoint(bin: &str) -> Option<PathBuf> {
    resolve_executable_path(bin).filter(|path| is_node_script(path))
}

fn resolve_executable_path(bin: &str) -> Option<PathBuf> {
    let direct = Path::new(bin);
    if direct.is_absolute() || direct.components().count() > 1 {
        return direct.canonicalize().ok();
    }

    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
}

fn is_node_script(path: &Path) -> bool {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension, "js" | "mjs" | "cjs"))
        .unwrap_or(false)
    {
        return true;
    }

    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut buffer = [0_u8; 128];
    let bytes = match file.read(&mut buffer) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let prefix = String::from_utf8_lossy(&buffer[..bytes]);
    prefix.starts_with("#!") && prefix.contains("node")
}

const PI_WRAPPER_SOURCE: &str = r#"
import { realpathSync } from "node:fs";
import { pathToFileURL } from "node:url";

const piBin = process.env.RALPH_PI_BIN;
if (!piBin) {
  throw new Error("RALPH_PI_BIN is required");
}
const piEntrypoint = process.env.RALPH_PI_ENTRYPOINT;
if (!piEntrypoint) {
  throw new Error("RALPH_PI_ENTRYPOINT is required");
}

Object.defineProperty(process, "title", {
  configurable: false,
  enumerable: true,
  get() {
    return "pi";
  },
  set(_) {}
});

process.argv = [process.argv[0], piBin, ...process.argv.slice(2)];
await import(pathToFileURL(realpathSync(piEntrypoint)).href);
"#;

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

impl PiResponseParser {
    /// Parse Pi JSON response format.
    pub(crate) fn parse_json(&self, json: &JsonValue) -> Option<String> {
        match json.get("type").and_then(|t| t.as_str()) {
            // Pi emits result objects in --mode json output.
            Some("result") => {
                let result = json.get("result")?;
                extract_text_content(result)
            }
            // Some Pi builds emit assistant output in message_end envelopes.
            Some("message_end") => {
                let message = json.get("message")?;
                if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                    return None;
                }
                let content = message.get("content")?;
                extract_text_content(content)
            }
            _ => None,
        }
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
