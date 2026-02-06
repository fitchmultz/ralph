//! Built-in runner plugin implementations.
//!
//! Responsibilities:
//! - Implement RunnerPlugin trait for all 7 built-in runners.
//! - Implement ResponseParser trait for each runner's JSON format.
//! - Encapsulate runner-specific CLI flag mapping and command building.
//!
//! Not handled here:
//! - External plugin protocol execution (see plugin.rs).
//! - Process execution and streaming (see process.rs).
//!
//! Invariants:
//! - Each built-in runner has a corresponding variant in BuiltInRunnerPlugin.
//! - Command builders must preserve temp guards until execution completes.

use std::path::Path;

use serde_json::Value as JsonValue;

use crate::commands::run::PhaseType;
use crate::constants::paths::{ENV_MODEL_USED, ENV_RUNNER_USED};
use crate::contracts::{Model, Runner};
use crate::runner::{
    ResolvedRunnerCliOptions, RunnerError, runner_execution_error,
    runner_execution_error_with_source,
};

use super::cli_spec::{
    apply_claude_options, apply_codex_global_options, apply_cursor_options, apply_gemini_options,
    apply_kimi_options, apply_pi_options,
};
use super::command::RunnerCommandBuilder;
use super::plugin_trait::{
    PluginCommandParts, ResponseParser, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};

/// Built-in runner plugin wrapper enum.
///
/// This enum wraps all 7 built-in runners and implements the RunnerPlugin trait
/// by delegating to runner-specific implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltInRunnerPlugin {
    Codex,
    Opencode,
    Gemini,
    Claude,
    Kimi,
    Pi,
    Cursor,
}

#[allow(dead_code)]
impl BuiltInRunnerPlugin {
    /// Returns the Runner enum variant for this plugin.
    pub fn runner(&self) -> Runner {
        match self {
            Self::Codex => Runner::Codex,
            Self::Opencode => Runner::Opencode,
            Self::Gemini => Runner::Gemini,
            Self::Claude => Runner::Claude,
            Self::Kimi => Runner::Kimi,
            Self::Pi => Runner::Pi,
            Self::Cursor => Runner::Cursor,
        }
    }

    /// Returns the runner ID string.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Gemini => "gemini",
            Self::Claude => "claude",
            Self::Kimi => "kimi",
            Self::Pi => "pi",
            Self::Cursor => "cursor",
        }
    }
}

impl RunnerPlugin for BuiltInRunnerPlugin {
    fn metadata(&self) -> RunnerMetadata {
        match self {
            Self::Codex => RunnerMetadata {
                id: "codex".to_string(),
                name: "OpenAI Codex CLI".to_string(),
                supports_resume: true,
                default_model: None,
            },
            Self::Opencode => RunnerMetadata {
                id: "opencode".to_string(),
                name: "Opencode".to_string(),
                supports_resume: true,
                default_model: None,
            },
            Self::Gemini => RunnerMetadata {
                id: "gemini".to_string(),
                name: "Google Gemini CLI".to_string(),
                supports_resume: true,
                default_model: None,
            },
            Self::Claude => RunnerMetadata {
                id: "claude".to_string(),
                name: "Anthropic Claude Code".to_string(),
                supports_resume: true,
                default_model: Some("sonnet".to_string()),
            },
            Self::Kimi => RunnerMetadata {
                id: "kimi".to_string(),
                name: "Kimi CLI".to_string(),
                supports_resume: true,
                default_model: None,
            },
            Self::Pi => RunnerMetadata {
                id: "pi".to_string(),
                name: "Pi Coding Agent".to_string(),
                supports_resume: true,
                default_model: None,
            },
            Self::Cursor => RunnerMetadata {
                id: "cursor".to_string(),
                name: "Cursor Agent".to_string(),
                supports_resume: true,
                default_model: None,
            },
        }
    }

    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError> {
        match self {
            Self::Codex => CodexPlugin.build_run_command(ctx),
            Self::Opencode => OpencodePlugin.build_run_command(ctx),
            Self::Gemini => GeminiPlugin.build_run_command(ctx),
            Self::Claude => ClaudePlugin.build_run_command(ctx),
            Self::Kimi => KimiPlugin.build_run_command(ctx),
            Self::Pi => PiPlugin.build_run_command(ctx),
            Self::Cursor => CursorPlugin.build_run_command(ctx),
        }
    }

    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError> {
        match self {
            Self::Codex => CodexPlugin.build_resume_command(ctx),
            Self::Opencode => OpencodePlugin.build_resume_command(ctx),
            Self::Gemini => GeminiPlugin.build_resume_command(ctx),
            Self::Claude => ClaudePlugin.build_resume_command(ctx),
            Self::Kimi => KimiPlugin.build_resume_command(ctx),
            Self::Pi => PiPlugin.build_resume_command(ctx),
            Self::Cursor => CursorPlugin.build_resume_command(ctx),
        }
    }

    fn parse_response_line(&self, line: &str, buffer: &mut String) -> Option<String> {
        let json = serde_json::from_str(line).ok()?;
        match self {
            Self::Codex => CodexResponseParser.parse_json(&json),
            Self::Opencode => OpencodeResponseParser.parse_json(&json, buffer),
            Self::Gemini => GeminiResponseParser.parse_json(&json),
            Self::Claude => ClaudeResponseParser.parse_json(&json),
            Self::Kimi => KimiResponseParser.parse_json(&json),
            Self::Pi => PiResponseParser.parse_json(&json),
            Self::Cursor => CursorResponseParser.parse_json(&json),
        }
    }

    fn requires_managed_session_id(&self) -> bool {
        matches!(self, Self::Kimi)
    }
}

// =============================================================================
// Codex Plugin
// =============================================================================

struct CodexPlugin;

impl RunnerPlugin for CodexPlugin {
    fn metadata(&self) -> RunnerMetadata {
        BuiltInRunnerPlugin::Codex.metadata()
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
        let json = serde_json::from_str(line).ok()?;
        CodexResponseParser.parse_json(&json)
    }
}

pub struct CodexResponseParser;

#[allow(dead_code)]
impl CodexResponseParser {
    fn parse_json(&self, json: &JsonValue) -> Option<String> {
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

// =============================================================================
// Opencode Plugin
// =============================================================================

struct OpencodePlugin;

impl RunnerPlugin for OpencodePlugin {
    fn metadata(&self) -> RunnerMetadata {
        BuiltInRunnerPlugin::Opencode.metadata()
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
        let json = serde_json::from_str(line).ok()?;
        OpencodeResponseParser.parse_json(&json, buffer)
    }
}

#[allow(dead_code)]
struct OpencodeResponseParser;

#[allow(dead_code)]
impl OpencodeResponseParser {
    fn parse_json(&self, json: &JsonValue, buffer: &mut String) -> Option<String> {
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

// =============================================================================
// Gemini Plugin
// =============================================================================

struct GeminiPlugin;

impl RunnerPlugin for GeminiPlugin {
    fn metadata(&self) -> RunnerMetadata {
        BuiltInRunnerPlugin::Gemini.metadata()
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
        let json = serde_json::from_str(line).ok()?;
        GeminiResponseParser.parse_json(&json)
    }
}

#[allow(dead_code)]
struct GeminiResponseParser;

#[allow(dead_code)]
impl GeminiResponseParser {
    fn parse_json(&self, json: &JsonValue) -> Option<String> {
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

// =============================================================================
// Claude Plugin
// =============================================================================

struct ClaudePlugin;

impl RunnerPlugin for ClaudePlugin {
    fn metadata(&self) -> RunnerMetadata {
        BuiltInRunnerPlugin::Claude.metadata()
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
        let json = serde_json::from_str(line).ok()?;
        ClaudeResponseParser.parse_json(&json)
    }
}

#[allow(dead_code)]
struct ClaudeResponseParser;

#[allow(dead_code)]
impl ClaudeResponseParser {
    fn parse_json(&self, json: &JsonValue) -> Option<String> {
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

// =============================================================================
// Kimi Plugin
// =============================================================================

struct KimiPlugin;

impl RunnerPlugin for KimiPlugin {
    fn metadata(&self) -> RunnerMetadata {
        BuiltInRunnerPlugin::Kimi.metadata()
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
        let json = serde_json::from_str(line).ok()?;
        KimiResponseParser.parse_json(&json)
    }

    fn requires_managed_session_id(&self) -> bool {
        true
    }
}

/// Response parser for Kimi's JSON format.
pub struct KimiResponseParser;

impl KimiResponseParser {
    fn parse_json(&self, json: &JsonValue) -> Option<String> {
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

// =============================================================================
// Pi Plugin
// =============================================================================

struct PiPlugin;

impl RunnerPlugin for PiPlugin {
    fn metadata(&self) -> RunnerMetadata {
        BuiltInRunnerPlugin::Pi.metadata()
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
        model: &Model,
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

fn resolve_pi_session_path(
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

fn pi_agent_root() -> Option<std::path::PathBuf> {
    if let Some(value) = std::env::var_os("PI_CODING_AGENT_DIR") {
        return Some(std::path::PathBuf::from(value));
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".pi").join("agent"))
}

fn pi_session_dir_name(work_dir: &Path) -> String {
    let mut path = work_dir.to_string_lossy().to_string();
    if let Some(stripped) = path.strip_prefix("\\\\?\\") {
        path = stripped.to_string();
    }
    let trimmed = path.trim_start_matches(['/', '\\']);
    let normalized = trimmed.replace(['/', '\\'], "-");
    format!("--{}--", normalized)
}

#[allow(dead_code)]
struct PiResponseParser;

#[allow(dead_code)]
impl PiResponseParser {
    fn parse_json(&self, json: &JsonValue) -> Option<String> {
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

// =============================================================================
// Cursor Plugin
// =============================================================================

struct CursorPlugin;

impl RunnerPlugin for CursorPlugin {
    fn metadata(&self) -> RunnerMetadata {
        BuiltInRunnerPlugin::Cursor.metadata()
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

#[allow(dead_code)]
struct CursorResponseParser;

#[allow(dead_code)]
impl CursorResponseParser {
    fn parse_json(&self, json: &JsonValue) -> Option<String> {
        // Cursor uses message_end format
        if json.get("type").and_then(|t| t.as_str()) != Some("message_end") {
            return None;
        }

        let message = json.get("message")?;
        if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            return None;
        }

        let content = message.get("content")?;
        extract_text_content(content)
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

// =============================================================================
// Shared Helpers
// =============================================================================

/// Apply analytics environment variables to track which runner/model was used.
fn apply_analytics_env(
    builder: RunnerCommandBuilder,
    runner: &Runner,
    model: &Model,
) -> RunnerCommandBuilder {
    builder
        .env(ENV_RUNNER_USED, runner.id())
        .env(ENV_MODEL_USED, model.as_str())
}

/// Extract text content from a JSON value (string or array of text objects).
#[allow(dead_code)]
fn extract_text_content(content: &JsonValue) -> Option<String> {
    match content {
        JsonValue::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        JsonValue::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
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
        _ => None,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn all_built_in_plugins_have_metadata() {
        let plugins = [
            BuiltInRunnerPlugin::Codex,
            BuiltInRunnerPlugin::Opencode,
            BuiltInRunnerPlugin::Gemini,
            BuiltInRunnerPlugin::Claude,
            BuiltInRunnerPlugin::Kimi,
            BuiltInRunnerPlugin::Pi,
            BuiltInRunnerPlugin::Cursor,
        ];

        for plugin in &plugins {
            let metadata = plugin.metadata();
            assert!(
                !metadata.id.is_empty(),
                "Plugin {:?} missing id",
                plugin.runner()
            );
            assert!(
                !metadata.name.is_empty(),
                "Plugin {:?} missing name",
                plugin.runner()
            );
            assert_eq!(metadata.id, plugin.id());
        }
    }

    #[test]
    fn kimi_requires_managed_session_id() {
        assert!(BuiltInRunnerPlugin::Kimi.requires_managed_session_id());
        assert!(!BuiltInRunnerPlugin::Codex.requires_managed_session_id());
        assert!(!BuiltInRunnerPlugin::Claude.requires_managed_session_id());
    }

    #[test]
    fn codex_response_parser_extracts_agent_message() {
        let parser = CodexResponseParser;
        let mut buffer = String::new();

        let json = serde_json::json!({
            "type": "item.completed",
            "item": {
                "type": "agent_message",
                "text": "Hello world"
            }
        });
        let result = parser.parse(&json, &mut buffer);

        assert_eq!(result, Some("Hello world".to_string()));
    }

    #[test]
    fn kimi_response_parser_extracts_assistant_text() {
        let parser = KimiResponseParser;
        let mut buffer = String::new();

        let json = serde_json::json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello from Kimi"}]
        });
        let result = parser.parse(&json, &mut buffer);

        assert_eq!(result, Some("Hello from Kimi".to_string()));
    }

    #[test]
    fn pi_session_dir_name_normalizes_path() {
        let name = pi_session_dir_name(Path::new("/Users/mitchfultz/Projects/AI/ralph"));
        assert_eq!(name, "--Users-mitchfultz-Projects-AI-ralph--");
    }

    #[test]
    fn extract_text_content_handles_string() {
        let json = JsonValue::String("  hello world  ".to_string());
        assert_eq!(extract_text_content(&json), Some("hello world".to_string()));
    }

    #[test]
    fn extract_text_content_handles_array() {
        let json = JsonValue::Array(vec![
            serde_json::json!({"type": "text", "text": "line 1"}),
            serde_json::json!({"type": "text", "text": "line 2"}),
        ]);
        assert_eq!(
            extract_text_content(&json),
            Some("line 1\nline 2".to_string())
        );
    }
}
