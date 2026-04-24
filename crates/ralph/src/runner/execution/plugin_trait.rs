//! Core traits for the runner plugin system.
//!
//! Purpose:
//! - Core traits for the runner plugin system.
//!
//! Responsibilities:
//! - Define the RunnerPlugin trait interface for all runner implementations.
//! - Define ResponseParser trait for runner-specific JSON parsing.
//! - Provide shared types for plugin metadata and execution context.
//!
//! Not handled here:
//! - Concrete runner implementations (see builtin_plugins.rs).
//! - External plugin protocol execution (see plugin.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - All built-in runners MUST implement RunnerPlugin via BuiltInRunnerPlugin enum.
//! - Trait methods are fallible; errors propagate to caller for handling.
//!
//! Note: Module-level dead_code allow is required because trait definitions and
//! context structs define fields that may not all be used by every implementation,
//! but are part of the public API contract for plugin implementors.

#![allow(dead_code)]

use std::path::Path;
use std::time::Duration;

use crate::commands::run::PhaseType;
use crate::contracts::{ClaudePermissionMode, Model, ReasoningEffort};
use crate::runner::{OutputHandler, OutputStream, ResolvedRunnerCliOptions, RunnerError};

/// Metadata about a runner plugin.
#[derive(Debug, Clone)]
pub struct RunnerMetadata {
    /// The runner identifier (e.g., "codex", "claude")
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Whether this runner supports session resumption
    pub supports_resume: bool,
    /// Default model when none specified
    pub default_model: Option<String>,
}

/// Context for building runner commands.
#[derive(Clone)]
pub struct RunContext<'a> {
    pub work_dir: &'a Path,
    pub bin: &'a str,
    pub model: Model,
    pub prompt: &'a str,
    pub timeout: Option<Duration>,
    pub output_handler: Option<OutputHandler>,
    pub output_stream: OutputStream,
    pub runner_cli: ResolvedRunnerCliOptions,
    /// Runner-specific settings
    pub reasoning_effort: Option<ReasoningEffort>,
    pub permission_mode: Option<ClaudePermissionMode>,
    pub phase_type: Option<PhaseType>,
    pub session_id: Option<String>,
}

impl std::fmt::Debug for RunContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunContext")
            .field("work_dir", &self.work_dir)
            .field("bin", &self.bin)
            .field("model", &self.model)
            .field("prompt", &self.prompt)
            .field("timeout", &self.timeout)
            .field("output_stream", &self.output_stream)
            .field("runner_cli", &self.runner_cli)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("permission_mode", &self.permission_mode)
            .field("phase_type", &self.phase_type)
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

/// Context for resuming a runner session.
#[derive(Clone)]
pub struct ResumeContext<'a> {
    pub work_dir: &'a Path,
    pub bin: &'a str,
    pub model: Model,
    pub session_id: &'a str,
    pub message: &'a str,
    pub timeout: Option<Duration>,
    pub output_handler: Option<OutputHandler>,
    pub output_stream: OutputStream,
    pub runner_cli: ResolvedRunnerCliOptions,
    /// Runner-specific settings
    pub reasoning_effort: Option<ReasoningEffort>,
    pub permission_mode: Option<ClaudePermissionMode>,
    pub phase_type: Option<PhaseType>,
}

impl std::fmt::Debug for ResumeContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResumeContext")
            .field("work_dir", &self.work_dir)
            .field("bin", &self.bin)
            .field("model", &self.model)
            .field("session_id", &self.session_id)
            .field("message", &self.message)
            .field("timeout", &self.timeout)
            .field("output_stream", &self.output_stream)
            .field("runner_cli", &self.runner_cli)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("permission_mode", &self.permission_mode)
            .field("phase_type", &self.phase_type)
            .finish_non_exhaustive()
    }
}

/// Type alias for the command parts returned by plugin command builders.
pub type PluginCommandParts = (
    std::process::Command,
    Option<Vec<u8>>,
    Vec<Box<dyn std::any::Any + Send + Sync>>,
);

/// The core trait for runner plugin implementations.
///
/// Both built-in runners and external plugins implement this trait.
pub trait RunnerPlugin: Send + Sync {
    /// Returns metadata about this runner.
    fn metadata(&self) -> RunnerMetadata;

    /// Builds the command for initial execution.
    ///
    /// Returns the command builder, stdin payload, and temp resource guards.
    fn build_run_command(&self, ctx: RunContext<'_>) -> Result<PluginCommandParts, RunnerError>;

    /// Builds the command for session resumption.
    ///
    /// Returns the command builder, stdin payload, and temp resource guards.
    fn build_resume_command(
        &self,
        ctx: ResumeContext<'_>,
    ) -> Result<PluginCommandParts, RunnerError>;

    /// Parses a line of JSON output and returns the assistant's response if found.
    ///
    /// Called line-by-line on the JSON stream.
    fn parse_response_line(&self, line: &str, buffer: &mut String) -> Option<String>;

    /// Returns true if this runner requires Ralph-managed session IDs.
    ///
    /// For example, Kimi doesn't emit session IDs in JSON, so Ralph must supply one.
    fn requires_managed_session_id(&self) -> bool {
        false
    }
}

/// Trait for runner-specific response parsers.
///
/// This is implemented per-runner to handle unique JSON formats.
pub trait ResponseParser: Send + Sync {
    /// Attempt to extract assistant text from a JSON value.
    ///
    /// Returns Some(text) if this value contains a complete or partial response.
    /// The buffer can be used for accumulating streaming responses.
    fn parse(&self, json: &serde_json::Value, buffer: &mut String) -> Option<String>;

    /// Returns the runner this parser handles.
    fn runner_id(&self) -> &str;
}
