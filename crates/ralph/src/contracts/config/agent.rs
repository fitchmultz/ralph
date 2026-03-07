//! Agent runner defaults configuration.
//!
//! Responsibilities:
//! - Define AgentConfig struct and merge behavior for runner defaults.
//! - Model CI gate execution using explicit argv or trusted shell settings.
//!
//! Not handled here:
//! - Runner-specific configuration (see `crate::contracts::runner`).
//! - Actual runner invocation (see `crate::runner` module).

use crate::contracts::config::{
    GitRevertMode, NotificationConfig, PhaseOverrides, RunnerRetryConfig, ScanPromptVersion,
    WebhookConfig,
};
use crate::contracts::model::{Model, ReasoningEffort};
use crate::contracts::runner::{ClaudePermissionMode, Runner, RunnerCliConfigRoot};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Platform shell mode for trusted CI gate execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShellMode {
    Posix,
    WindowsCmd,
}

impl ShellMode {
    pub fn display_prefix(self) -> &'static str {
        match self {
            Self::Posix => "sh -c",
            Self::WindowsCmd => "cmd /C",
        }
    }
}

/// Trusted shell execution settings for the CI gate.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ShellCommandConfig {
    /// Shell mode used to interpret the command string.
    pub mode: Option<ShellMode>,

    /// Command string evaluated by the configured shell.
    pub command: Option<String>,
}

impl ShellCommandConfig {
    pub fn merge_from(&mut self, other: Self) {
        if other.mode.is_some() {
            self.mode = other.mode;
        }
        if other.command.is_some() {
            self.command = other.command;
        }
    }
}

/// Structured CI gate execution settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct CiGateConfig {
    /// Enable or disable the CI gate entirely.
    pub enabled: Option<bool>,

    /// Direct argv execution. The first item is the program and remaining items are arguments.
    pub argv: Option<Vec<String>>,

    /// Explicit shell-mode execution. Intended only for trusted local configuration.
    pub shell: Option<ShellCommandConfig>,
}

impl CiGateConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn display_string(&self) -> String {
        if !self.is_enabled() {
            return "disabled".to_string();
        }

        if let Some(argv) = &self.argv {
            return format_argv(argv);
        }

        if let Some(shell) = &self.shell {
            let mode = shell
                .mode
                .map(ShellMode::display_prefix)
                .unwrap_or("<shell>");
            let command = shell.command.as_deref().unwrap_or("<unset>");
            return format!("{mode} {command}");
        }

        "<unset>".to_string()
    }

    pub fn merge_from(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
        if other.argv.is_some() {
            self.argv = other.argv;
        }
        if let Some(other_shell) = other.shell {
            match &mut self.shell {
                Some(existing) => existing.merge_from(other_shell),
                None => self.shell = Some(other_shell),
            }
        }
    }
}

fn format_argv(argv: &[String]) -> String {
    argv.iter()
        .map(|part| {
            if part.is_empty() {
                "\"\"".to_string()
            } else if part
                .chars()
                .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '\\'))
            {
                format!("{part:?}")
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Agent runner defaults (Claude, Codex, OpenCode, Gemini, or Cursor).
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    /// Which harness to use by default.
    pub runner: Option<Runner>,

    /// Default model.
    pub model: Option<Model>,

    /// Default reasoning effort (only meaningful for Codex models).
    pub reasoning_effort: Option<ReasoningEffort>,

    /// Number of iterations to run for each task (default: 1).
    #[schemars(range(min = 1))]
    pub iterations: Option<u8>,

    /// Reasoning effort override for follow-up iterations (iterations > 1).
    /// Only meaningful for Codex models.
    pub followup_reasoning_effort: Option<ReasoningEffort>,

    /// Override the codex executable name/path (default is "codex" if None).
    pub codex_bin: Option<String>,

    /// Override the opencode executable name/path (default is "opencode" if None).
    pub opencode_bin: Option<String>,

    /// Override the gemini executable name/path (default is "gemini" if None).
    pub gemini_bin: Option<String>,

    /// Override the claude executable name/path (default is "claude" if None).
    pub claude_bin: Option<String>,

    /// Override the cursor agent executable name/path (default is "agent" if None).
    ///
    /// NOTE: Cursor's runner binary name is `agent` (not `cursor`).
    pub cursor_bin: Option<String>,

    /// Override the kimi executable name/path (default is "kimi" if None).
    pub kimi_bin: Option<String>,

    /// Override the pi executable name/path (default is "pi" if None).
    pub pi_bin: Option<String>,

    /// Claude permission mode for tool and edit approval.
    /// AcceptEdits: auto-approves file edits only
    /// BypassPermissions: skip all permission prompts (YOLO mode)
    pub claude_permission_mode: Option<ClaudePermissionMode>,

    /// Normalized runner CLI behavior overrides (output/approval/sandbox/etc).
    ///
    /// This is additive: existing runner-specific fields remain supported.
    pub runner_cli: Option<RunnerCliConfigRoot>,

    /// Per-phase overrides for runner, model, and reasoning effort.
    ///
    /// Allows specifying different settings for each phase (1, 2, 3).
    /// Phase-specific values override the global agent settings.
    pub phase_overrides: Option<PhaseOverrides>,

    /// Additional instruction files to inject at the top of every prompt sent to runner CLIs.
    ///
    /// Paths may be absolute, `~/`-prefixed, or repo-root relative. Missing files are treated as
    /// configuration errors. To include repo-local AGENTS.md, add `"AGENTS.md"` to this list.
    pub instruction_files: Option<Vec<PathBuf>>,

    /// Require RepoPrompt usage during planning (inject context_builder instructions).
    pub repoprompt_plan_required: Option<bool>,

    /// Inject RepoPrompt tooling reminder block into prompts.
    pub repoprompt_tool_injection: Option<bool>,

    /// Structured CI gate execution settings.
    pub ci_gate: Option<CiGateConfig>,

    /// Controls automatic git revert behavior when runner or supervision errors occur.
    pub git_revert_mode: Option<GitRevertMode>,

    /// Enable automatic git commit and push after successful runs (default: true).
    pub git_commit_push_enabled: Option<bool>,

    /// Number of execution phases (1, 2, or 3).
    /// 1 = single-pass, 2 = plan+implement, 3 = plan+implement+review.
    #[schemars(range(min = 1, max = 3))]
    pub phases: Option<u8>,

    /// Desktop notification configuration for task completion.
    pub notification: NotificationConfig,

    /// Webhook configuration for HTTP task event notifications.
    pub webhook: WebhookConfig,

    /// Session timeout in hours for crash recovery (default: 24).
    /// Sessions older than this threshold are considered stale and require
    /// explicit user confirmation to resume.
    #[schemars(range(min = 1))]
    pub session_timeout_hours: Option<u64>,

    /// Scan prompt version to use (v1 or v2, default: v2).
    pub scan_prompt_version: Option<ScanPromptVersion>,

    /// Runner invocation retry/backoff configuration.
    pub runner_retry: RunnerRetryConfig,
}

impl AgentConfig {
    pub fn ci_gate_enabled(&self) -> bool {
        self.ci_gate
            .as_ref()
            .map(CiGateConfig::is_enabled)
            .unwrap_or(true)
    }

    pub fn ci_gate_display_string(&self) -> String {
        self.ci_gate
            .as_ref()
            .map(CiGateConfig::display_string)
            .unwrap_or_else(|| "make ci".to_string())
    }

    pub fn merge_from(&mut self, other: Self) {
        if other.runner.is_some() {
            self.runner = other.runner;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.reasoning_effort.is_some() {
            self.reasoning_effort = other.reasoning_effort;
        }
        if other.iterations.is_some() {
            self.iterations = other.iterations;
        }
        if other.followup_reasoning_effort.is_some() {
            self.followup_reasoning_effort = other.followup_reasoning_effort;
        }
        if other.codex_bin.is_some() {
            self.codex_bin = other.codex_bin;
        }
        if other.opencode_bin.is_some() {
            self.opencode_bin = other.opencode_bin;
        }
        if other.gemini_bin.is_some() {
            self.gemini_bin = other.gemini_bin;
        }
        if other.claude_bin.is_some() {
            self.claude_bin = other.claude_bin;
        }
        if other.cursor_bin.is_some() {
            self.cursor_bin = other.cursor_bin;
        }
        if other.kimi_bin.is_some() {
            self.kimi_bin = other.kimi_bin;
        }
        if other.pi_bin.is_some() {
            self.pi_bin = other.pi_bin;
        }
        if other.phases.is_some() {
            self.phases = other.phases;
        }
        if other.claude_permission_mode.is_some() {
            self.claude_permission_mode = other.claude_permission_mode;
        }
        if let Some(other_runner_cli) = other.runner_cli {
            match &mut self.runner_cli {
                Some(existing) => existing.merge_from(other_runner_cli),
                None => self.runner_cli = Some(other_runner_cli),
            }
        }
        if let Some(other_phase_overrides) = other.phase_overrides {
            match &mut self.phase_overrides {
                Some(existing) => existing.merge_from(other_phase_overrides),
                None => self.phase_overrides = Some(other_phase_overrides),
            }
        }
        if other.instruction_files.is_some() {
            self.instruction_files = other.instruction_files;
        }
        if other.repoprompt_plan_required.is_some() {
            self.repoprompt_plan_required = other.repoprompt_plan_required;
        }
        if other.repoprompt_tool_injection.is_some() {
            self.repoprompt_tool_injection = other.repoprompt_tool_injection;
        }
        if let Some(other_ci_gate) = other.ci_gate {
            match &mut self.ci_gate {
                Some(existing) => existing.merge_from(other_ci_gate),
                None => self.ci_gate = Some(other_ci_gate),
            }
        }
        if other.git_revert_mode.is_some() {
            self.git_revert_mode = other.git_revert_mode;
        }
        if other.git_commit_push_enabled.is_some() {
            self.git_commit_push_enabled = other.git_commit_push_enabled;
        }
        self.notification.merge_from(other.notification);
        self.webhook.merge_from(other.webhook);
        if other.session_timeout_hours.is_some() {
            self.session_timeout_hours = other.session_timeout_hours;
        }
        if other.scan_prompt_version.is_some() {
            self.scan_prompt_version = other.scan_prompt_version;
        }
        self.runner_retry.merge_from(other.runner_retry);
    }
}
