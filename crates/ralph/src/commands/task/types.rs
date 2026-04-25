//! Shared task-command option and output types.
//!
//! Purpose:
//! - Define reusable option and output types shared across task build, update, refactor, and decomposition flows.
//!
//! Responsibilities:
//! - Hold stable task-command structs and enums that are consumed by task submodules and CLI entrypoints.
//! - Centralize output-routing helpers for runner-backed task build workflows.
//!
//! Not handled here:
//! - Runner setting resolution.
//! - Request parsing.
//! - Queue mutation logic.
//!
//! Usage:
//! - Imported by task command helpers, CLI adapters, and tests that need canonical task command types.
//!
//! Invariants/assumptions:
//! - Output routing semantics stay aligned with `runner::OutputStream` and optional output handlers.
//! - These types remain data-only and do not own task-building business logic.

use crate::contracts::{Model, ReasoningEffort, Runner, RunnerCliOptionsPatch};
use crate::runner;
use std::path::PathBuf;

/// Batching mode for grouping related files in build-refactor.
#[derive(Clone, Copy, Debug)]
pub enum BatchMode {
    /// Group files in same directory with similar names (e.g., test files with source).
    Auto,
    /// Create individual task per file.
    Never,
    /// Group all files in same module/directory.
    Aggressive,
}

impl From<crate::cli::task::BatchMode> for BatchMode {
    fn from(mode: crate::cli::task::BatchMode) -> Self {
        match mode {
            crate::cli::task::BatchMode::Auto => BatchMode::Auto,
            crate::cli::task::BatchMode::Never => BatchMode::Never,
            crate::cli::task::BatchMode::Aggressive => BatchMode::Aggressive,
        }
    }
}

/// Options for the build-refactor command.
pub struct TaskBuildRefactorOptions {
    pub threshold: usize,
    pub path: Option<PathBuf>,
    pub dry_run: bool,
    pub batch: BatchMode,
    pub extra_tags: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
}

/// Canonical destination for runner output during task build workflows.
pub enum TaskBuildOutputTarget {
    /// Stream runner output directly to stdout/stderr for human CLI use.
    Terminal,
    /// Suppress direct output so stdout remains reserved for machine JSON.
    Quiet,
    /// Deliver output to an app/event handler without writing to stdout/stderr.
    Handler(runner::OutputHandler),
}

impl TaskBuildOutputTarget {
    pub(crate) fn output_handler(&self) -> Option<runner::OutputHandler> {
        match self {
            TaskBuildOutputTarget::Terminal | TaskBuildOutputTarget::Quiet => None,
            TaskBuildOutputTarget::Handler(handler) => Some(handler.clone()),
        }
    }

    pub(crate) fn output_stream(&self) -> runner::OutputStream {
        match self {
            TaskBuildOutputTarget::Terminal => runner::OutputStream::Terminal,
            TaskBuildOutputTarget::Quiet | TaskBuildOutputTarget::Handler(_) => {
                runner::OutputStream::HandlerOnly
            }
        }
    }
}

// TaskBuildOptions controls runner-driven task creation via .ralph/prompts/task_builder.md.
pub struct TaskBuildOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
    /// Single source of truth for runner output routing.
    pub output: TaskBuildOutputTarget,
    /// Optional template name to use as a base for task fields
    pub template_hint: Option<String>,
    /// Optional target path for template variable substitution
    pub template_target: Option<String>,
    /// Fail on unknown template variables (default: false, warns only)
    pub strict_templates: bool,
    /// Estimated minutes for task completion
    pub estimated_minutes: Option<u32>,
}

// TaskUpdateSettings controls runner-driven task updates via .ralph/prompts/task_updater.md.
pub struct TaskUpdateSettings {
    pub fields: String,
    pub runner_override: Option<Runner>,
    pub model_override: Option<Model>,
    pub reasoning_effort_override: Option<ReasoningEffort>,
    pub runner_cli_overrides: RunnerCliOptionsPatch,
    pub force: bool,
    pub repoprompt_tool_injection: bool,
    pub dry_run: bool,
}
