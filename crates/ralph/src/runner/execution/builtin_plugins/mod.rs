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

use serde_json::Value as JsonValue;

use crate::constants::paths::{ENV_MODEL_USED, ENV_RUNNER_USED};
use crate::contracts::{Model, Runner};
use crate::runner::RunnerError;

use super::command::RunnerCommandBuilder;
use super::plugin_trait::{
    PluginCommandParts, ResumeContext, RunContext, RunnerMetadata, RunnerPlugin,
};

// Submodules for each runner implementation
pub mod claude;
pub mod codex;
pub mod cursor;
pub mod gemini;
pub mod kimi;
pub mod opencode;
pub mod pi;

// Re-export all plugin types
pub use claude::{ClaudePlugin, ClaudeResponseParser};
pub use codex::{CodexPlugin, CodexResponseParser};
pub use cursor::{CursorPlugin, CursorResponseParser};
pub use gemini::{GeminiPlugin, GeminiResponseParser};
pub use kimi::{KimiPlugin, KimiResponseParser};
pub use opencode::{OpencodePlugin, OpencodeResponseParser};
pub use pi::{PiPlugin, PiResponseParser};

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
// Shared Helpers
// =============================================================================

/// Apply analytics environment variables to track which runner/model was used.
pub(crate) fn apply_analytics_env(
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
pub(crate) fn extract_text_content(content: &JsonValue) -> Option<String> {
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

#[cfg(test)]
mod tests;
