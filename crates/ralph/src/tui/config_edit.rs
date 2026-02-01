//! TUI project config editor helpers.
//!
//! Responsibilities:
//! - Define which config fields the TUI can display/edit in the "project config" overlay.
//! - Provide display/cycle/clear behavior for supported config keys.
//!
//! Does not handle:
//! - Loading/saving config files (handled by `crate::config` and `tui::app`).
//! - Full-fidelity config editing (this editor intentionally covers a curated subset).
//!
//! Invariants/assumptions:
//! - Edits apply to the *project* config layer (`.ralph/config.json`) as leaf-wise overrides.
//! - Fields cycle through a fixed set of allowed values plus an "unset" state.

use super::app::App;
use crate::contracts::{
    ClaudePermissionMode, GitRevertMode, Model, ProjectType, ReasoningEffort, Runner,
    RunnerApprovalMode, RunnerCliConfigRoot, RunnerCliOptionsPatch, RunnerOutputFormat,
    RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity, UnsupportedOptionPolicy,
};
use anyhow::{Result, anyhow, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFieldKind {
    Cycle,
    Toggle,
    Text,
}

/// Risk level for a config field, used to display warnings and require confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// No special risk; no warning needed.
    None,
    /// Warning level; show inline explanation.
    Warning,
    /// Danger level; show inline explanation and require confirmation when enabling.
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    ProjectType,
    QueueFile,
    QueueDoneFile,
    QueueIdPrefix,
    QueueIdWidth,
    AgentRunner,
    AgentModel,
    AgentReasoningEffort,
    AgentIterations,
    AgentFollowupReasoningEffort,
    AgentCodexBin,
    AgentOpencodeBin,
    AgentGeminiBin,
    AgentClaudeBin,
    AgentCursorBin,
    AgentClaudePermissionMode,
    AgentRunnerCliOutputFormat,
    AgentRunnerCliVerbosity,
    AgentRunnerCliApprovalMode,
    AgentRunnerCliSandboxMode,
    AgentRunnerCliPlanMode,
    AgentRunnerCliUnsupportedOptionPolicy,
    AgentRepopromptPlanRequired,
    AgentRepopromptToolInjection,
    AgentGitRevertMode,
    AgentGitCommitPushEnabled,
    AgentPhases,
}

#[derive(Debug, Clone)]
pub struct ConfigEntry {
    pub key: ConfigKey,
    pub label: &'static str,
    pub value: String,
    pub kind: ConfigFieldKind,
    pub risk_level: RiskLevel,
    pub description: &'static str,
}

impl App {
    pub(crate) fn config_entries(&self) -> Vec<ConfigEntry> {
        vec![
            ConfigEntry {
                key: ConfigKey::ProjectType,
                label: "project_type",
                value: display_project_type(self.project_config.project_type),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::QueueFile,
                label: "queue.file",
                value: display_path(self.project_config.queue.file.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::QueueDoneFile,
                label: "queue.done_file",
                value: display_path(self.project_config.queue.done_file.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::QueueIdPrefix,
                label: "queue.id_prefix",
                value: display_string(self.project_config.queue.id_prefix.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::QueueIdWidth,
                label: "queue.id_width",
                value: display_u8(self.project_config.queue.id_width),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentRunner,
                label: "agent.runner",
                value: display_runner(self.project_config.agent.runner),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentModel,
                label: "agent.model",
                value: display_model(self.project_config.agent.model.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentReasoningEffort,
                label: "agent.reasoning_effort",
                value: display_reasoning_effort(self.project_config.agent.reasoning_effort),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentIterations,
                label: "agent.iterations",
                value: display_u8(self.project_config.agent.iterations),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentFollowupReasoningEffort,
                label: "agent.followup_reasoning_effort",
                value: display_reasoning_effort(
                    self.project_config.agent.followup_reasoning_effort,
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentCodexBin,
                label: "agent.codex_bin",
                value: display_string(self.project_config.agent.codex_bin.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentOpencodeBin,
                label: "agent.opencode_bin",
                value: display_string(self.project_config.agent.opencode_bin.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentGeminiBin,
                label: "agent.gemini_bin",
                value: display_string(self.project_config.agent.gemini_bin.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentClaudeBin,
                label: "agent.claude_bin",
                value: display_string(self.project_config.agent.claude_bin.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentCursorBin,
                label: "agent.cursor_bin",
                value: display_string(self.project_config.agent.cursor_bin.as_ref()),
                kind: ConfigFieldKind::Text,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentClaudePermissionMode,
                label: "agent.claude_permission_mode",
                value: display_claude_permission_mode(
                    self.project_config.agent.claude_permission_mode,
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::Warning,
                description: "bypass_permissions allows unchecked edits",
            },
            ConfigEntry {
                key: ConfigKey::AgentRunnerCliOutputFormat,
                label: "agent.runner_cli.defaults.output_format",
                value: display_runner_output_format(
                    self.project_config
                        .agent
                        .runner_cli
                        .as_ref()
                        .and_then(|root| root.defaults.output_format),
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentRunnerCliVerbosity,
                label: "agent.runner_cli.defaults.verbosity",
                value: display_runner_verbosity(
                    self.project_config
                        .agent
                        .runner_cli
                        .as_ref()
                        .and_then(|root| root.defaults.verbosity),
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentRunnerCliApprovalMode,
                label: "agent.runner_cli.defaults.approval_mode",
                value: display_runner_approval_mode(
                    self.project_config
                        .agent
                        .runner_cli
                        .as_ref()
                        .and_then(|root| root.defaults.approval_mode),
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::Warning,
                description: "yolo bypasses all approval prompts",
            },
            ConfigEntry {
                key: ConfigKey::AgentRunnerCliSandboxMode,
                label: "agent.runner_cli.defaults.sandbox",
                value: display_runner_sandbox_mode(
                    self.project_config
                        .agent
                        .runner_cli
                        .as_ref()
                        .and_then(|root| root.defaults.sandbox),
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentRunnerCliPlanMode,
                label: "agent.runner_cli.defaults.plan_mode",
                value: display_runner_plan_mode(
                    self.project_config
                        .agent
                        .runner_cli
                        .as_ref()
                        .and_then(|root| root.defaults.plan_mode),
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentRunnerCliUnsupportedOptionPolicy,
                label: "agent.runner_cli.defaults.unsupported_option_policy",
                value: display_unsupported_option_policy(
                    self.project_config
                        .agent
                        .runner_cli
                        .as_ref()
                        .and_then(|root| root.defaults.unsupported_option_policy),
                ),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentRepopromptPlanRequired,
                label: "agent.repoprompt_plan_required",
                value: display_bool(self.project_config.agent.repoprompt_plan_required),
                kind: ConfigFieldKind::Toggle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentRepopromptToolInjection,
                label: "agent.repoprompt_tool_injection",
                value: display_bool(self.project_config.agent.repoprompt_tool_injection),
                kind: ConfigFieldKind::Toggle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentGitRevertMode,
                label: "agent.git_revert_mode",
                value: display_git_revert_mode(self.project_config.agent.git_revert_mode),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
            ConfigEntry {
                key: ConfigKey::AgentGitCommitPushEnabled,
                label: "agent.git_commit_push_enabled",
                value: display_bool(self.project_config.agent.git_commit_push_enabled),
                kind: ConfigFieldKind::Toggle,
                risk_level: RiskLevel::Danger,
                description: "auto-pushes changes to remote",
            },
            ConfigEntry {
                key: ConfigKey::AgentPhases,
                label: "agent.phases",
                value: display_u8(self.project_config.agent.phases),
                kind: ConfigFieldKind::Cycle,
                risk_level: RiskLevel::None,
                description: "",
            },
        ]
    }

    pub(crate) fn config_value_for_edit(&self, key: ConfigKey) -> String {
        match key {
            ConfigKey::QueueFile => self
                .project_config
                .queue
                .file
                .as_ref()
                .map(|p: &PathBuf| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            ConfigKey::QueueDoneFile => self
                .project_config
                .queue
                .done_file
                .as_ref()
                .map(|p: &PathBuf| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            ConfigKey::QueueIdPrefix => self
                .project_config
                .queue
                .id_prefix
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::QueueIdWidth => self
                .project_config
                .queue
                .id_width
                .map(|v: u8| v.to_string())
                .unwrap_or_default(),
            ConfigKey::AgentModel => self
                .project_config
                .agent
                .model
                .as_ref()
                .map(|v: &Model| v.as_str().to_string())
                .unwrap_or_default(),
            ConfigKey::AgentIterations => self
                .project_config
                .agent
                .iterations
                .map(|value: u8| value.to_string())
                .unwrap_or_default(),
            ConfigKey::AgentCodexBin => self
                .project_config
                .agent
                .codex_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::AgentOpencodeBin => self
                .project_config
                .agent
                .opencode_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::AgentGeminiBin => self
                .project_config
                .agent
                .gemini_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::AgentClaudeBin => self
                .project_config
                .agent
                .claude_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            ConfigKey::AgentCursorBin => self
                .project_config
                .agent
                .cursor_bin
                .as_ref()
                .cloned()
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    pub(crate) fn apply_config_text_value(&mut self, key: ConfigKey, input: &str) -> Result<()> {
        let trimmed = input.trim();
        match key {
            ConfigKey::QueueFile => {
                self.project_config.queue.file = if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                };
            }
            ConfigKey::QueueDoneFile => {
                self.project_config.queue.done_file = if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                };
            }
            ConfigKey::QueueIdPrefix => {
                self.project_config.queue.id_prefix = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::QueueIdWidth => {
                self.project_config.queue.id_width = if trimmed.is_empty() {
                    None
                } else {
                    let value: u8 = trimmed
                        .parse()
                        .map_err(|_| anyhow!("queue.id_width must be a valid number (e.g., 4)"))?;
                    if value == 0 {
                        bail!("queue.id_width must be greater than 0");
                    }
                    Some(value)
                };
            }
            ConfigKey::AgentModel => {
                self.project_config.agent.model = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.parse::<Model>().map_err(|msg| anyhow!(msg))?)
                };
            }
            ConfigKey::AgentIterations => {
                self.project_config.agent.iterations = if trimmed.is_empty() {
                    None
                } else {
                    let value: u8 = trimmed.parse().map_err(|_| {
                        anyhow!("agent.iterations must be a valid number (e.g., 1)")
                    })?;
                    if value == 0 {
                        bail!("agent.iterations must be greater than 0");
                    }
                    Some(value)
                };
            }
            ConfigKey::AgentCodexBin => {
                self.project_config.agent.codex_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::AgentOpencodeBin => {
                self.project_config.agent.opencode_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::AgentGeminiBin => {
                self.project_config.agent.gemini_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::AgentClaudeBin => {
                self.project_config.agent.claude_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            ConfigKey::AgentCursorBin => {
                self.project_config.agent.cursor_bin = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
            }
            _ => {}
        }
        self.dirty_config = true;
        Ok(())
    }

    pub(crate) fn cycle_config_value(&mut self, key: ConfigKey) {
        match key {
            ConfigKey::ProjectType => {
                self.project_config.project_type =
                    cycle_project_type(self.project_config.project_type);
            }
            ConfigKey::AgentRunner => {
                self.project_config.agent.runner = cycle_runner(self.project_config.agent.runner);
            }
            ConfigKey::AgentReasoningEffort => {
                self.project_config.agent.reasoning_effort =
                    cycle_reasoning_effort(self.project_config.agent.reasoning_effort);
            }
            ConfigKey::AgentFollowupReasoningEffort => {
                self.project_config.agent.followup_reasoning_effort =
                    cycle_reasoning_effort(self.project_config.agent.followup_reasoning_effort);
            }
            ConfigKey::AgentClaudePermissionMode => {
                self.project_config.agent.claude_permission_mode =
                    cycle_claude_permission_mode(self.project_config.agent.claude_permission_mode);
            }
            ConfigKey::AgentRunnerCliOutputFormat => {
                let defaults = runner_cli_defaults_mut(&mut self.project_config.agent);
                defaults.output_format = cycle_runner_output_format(defaults.output_format);
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliVerbosity => {
                let defaults = runner_cli_defaults_mut(&mut self.project_config.agent);
                defaults.verbosity = cycle_runner_verbosity(defaults.verbosity);
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliApprovalMode => {
                let defaults = runner_cli_defaults_mut(&mut self.project_config.agent);
                defaults.approval_mode = cycle_runner_approval_mode(defaults.approval_mode);
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliSandboxMode => {
                let defaults = runner_cli_defaults_mut(&mut self.project_config.agent);
                defaults.sandbox = cycle_runner_sandbox_mode(defaults.sandbox);
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliPlanMode => {
                let defaults = runner_cli_defaults_mut(&mut self.project_config.agent);
                defaults.plan_mode = cycle_runner_plan_mode(defaults.plan_mode);
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliUnsupportedOptionPolicy => {
                let defaults = runner_cli_defaults_mut(&mut self.project_config.agent);
                defaults.unsupported_option_policy =
                    cycle_unsupported_option_policy(defaults.unsupported_option_policy);
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRepopromptPlanRequired => {
                self.project_config.agent.repoprompt_plan_required =
                    cycle_bool(self.project_config.agent.repoprompt_plan_required);
            }
            ConfigKey::AgentRepopromptToolInjection => {
                self.project_config.agent.repoprompt_tool_injection =
                    cycle_bool(self.project_config.agent.repoprompt_tool_injection);
            }
            ConfigKey::AgentGitRevertMode => {
                self.project_config.agent.git_revert_mode =
                    cycle_git_revert_mode(self.project_config.agent.git_revert_mode);
            }
            ConfigKey::AgentGitCommitPushEnabled => {
                self.project_config.agent.git_commit_push_enabled =
                    cycle_bool(self.project_config.agent.git_commit_push_enabled);
            }
            ConfigKey::AgentPhases => {
                self.project_config.agent.phases = cycle_phases(self.project_config.agent.phases);
            }
            _ => {}
        }
        self.dirty_config = true;
    }

    pub(crate) fn clear_config_value(&mut self, key: ConfigKey) {
        match key {
            ConfigKey::ProjectType => self.project_config.project_type = None,
            ConfigKey::QueueFile => self.project_config.queue.file = None,
            ConfigKey::QueueDoneFile => self.project_config.queue.done_file = None,
            ConfigKey::QueueIdPrefix => self.project_config.queue.id_prefix = None,
            ConfigKey::QueueIdWidth => self.project_config.queue.id_width = None,
            ConfigKey::AgentRunner => self.project_config.agent.runner = None,
            ConfigKey::AgentModel => self.project_config.agent.model = None,
            ConfigKey::AgentReasoningEffort => self.project_config.agent.reasoning_effort = None,
            ConfigKey::AgentIterations => self.project_config.agent.iterations = None,
            ConfigKey::AgentFollowupReasoningEffort => {
                self.project_config.agent.followup_reasoning_effort = None;
            }
            ConfigKey::AgentCodexBin => self.project_config.agent.codex_bin = None,
            ConfigKey::AgentOpencodeBin => self.project_config.agent.opencode_bin = None,
            ConfigKey::AgentGeminiBin => self.project_config.agent.gemini_bin = None,
            ConfigKey::AgentClaudeBin => self.project_config.agent.claude_bin = None,
            ConfigKey::AgentCursorBin => self.project_config.agent.cursor_bin = None,
            ConfigKey::AgentClaudePermissionMode => {
                self.project_config.agent.claude_permission_mode = None;
            }
            ConfigKey::AgentRunnerCliOutputFormat => {
                if let Some(root) = &mut self.project_config.agent.runner_cli {
                    root.defaults.output_format = None;
                }
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliVerbosity => {
                if let Some(root) = &mut self.project_config.agent.runner_cli {
                    root.defaults.verbosity = None;
                }
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliApprovalMode => {
                if let Some(root) = &mut self.project_config.agent.runner_cli {
                    root.defaults.approval_mode = None;
                }
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliSandboxMode => {
                if let Some(root) = &mut self.project_config.agent.runner_cli {
                    root.defaults.sandbox = None;
                }
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliPlanMode => {
                if let Some(root) = &mut self.project_config.agent.runner_cli {
                    root.defaults.plan_mode = None;
                }
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRunnerCliUnsupportedOptionPolicy => {
                if let Some(root) = &mut self.project_config.agent.runner_cli {
                    root.defaults.unsupported_option_policy = None;
                }
                prune_runner_cli_root(&mut self.project_config.agent);
            }
            ConfigKey::AgentRepopromptPlanRequired => {
                self.project_config.agent.repoprompt_plan_required = None
            }
            ConfigKey::AgentRepopromptToolInjection => {
                self.project_config.agent.repoprompt_tool_injection = None
            }
            ConfigKey::AgentGitRevertMode => self.project_config.agent.git_revert_mode = None,
            ConfigKey::AgentGitCommitPushEnabled => {
                self.project_config.agent.git_commit_push_enabled = None
            }
            ConfigKey::AgentPhases => self.project_config.agent.phases = None,
        }
        self.dirty_config = true;
    }
}

fn default_config_value() -> String {
    "(global default)".to_string()
}

fn display_project_type(value: Option<ProjectType>) -> String {
    match value {
        Some(ProjectType::Code) => "code".to_string(),
        Some(ProjectType::Docs) => "docs".to_string(),
        None => default_config_value(),
    }
}

fn display_runner(value: Option<Runner>) -> String {
    match value {
        Some(Runner::Codex) => "codex".to_string(),
        Some(Runner::Opencode) => "opencode".to_string(),
        Some(Runner::Gemini) => "gemini".to_string(),
        Some(Runner::Claude) => "claude".to_string(),
        Some(Runner::Cursor) => "cursor".to_string(),
        Some(Runner::Kimi) => "kimi".to_string(),
        Some(Runner::Pi) => "pi".to_string(),
        None => default_config_value(),
    }
}

fn display_reasoning_effort(value: Option<ReasoningEffort>) -> String {
    match value {
        Some(ReasoningEffort::Low) => "low".to_string(),
        Some(ReasoningEffort::Medium) => "medium".to_string(),
        Some(ReasoningEffort::High) => "high".to_string(),
        Some(ReasoningEffort::XHigh) => "xhigh".to_string(),
        None => default_config_value(),
    }
}

fn display_claude_permission_mode(value: Option<ClaudePermissionMode>) -> String {
    match value {
        Some(ClaudePermissionMode::AcceptEdits) => "accept_edits".to_string(),
        Some(ClaudePermissionMode::BypassPermissions) => "bypass_permissions".to_string(),
        None => default_config_value(),
    }
}

fn display_runner_output_format(value: Option<RunnerOutputFormat>) -> String {
    match value {
        Some(RunnerOutputFormat::StreamJson) => "stream_json".to_string(),
        Some(RunnerOutputFormat::Json) => "json".to_string(),
        Some(RunnerOutputFormat::Text) => "text".to_string(),
        None => default_config_value(),
    }
}

fn display_runner_verbosity(value: Option<RunnerVerbosity>) -> String {
    match value {
        Some(RunnerVerbosity::Quiet) => "quiet".to_string(),
        Some(RunnerVerbosity::Normal) => "normal".to_string(),
        Some(RunnerVerbosity::Verbose) => "verbose".to_string(),
        None => default_config_value(),
    }
}

fn display_runner_approval_mode(value: Option<RunnerApprovalMode>) -> String {
    match value {
        Some(RunnerApprovalMode::Default) => "default".to_string(),
        Some(RunnerApprovalMode::AutoEdits) => "auto_edits".to_string(),
        Some(RunnerApprovalMode::Yolo) => "yolo".to_string(),
        Some(RunnerApprovalMode::Safe) => "safe".to_string(),
        None => default_config_value(),
    }
}

fn display_runner_sandbox_mode(value: Option<RunnerSandboxMode>) -> String {
    match value {
        Some(RunnerSandboxMode::Default) => "default".to_string(),
        Some(RunnerSandboxMode::Enabled) => "enabled".to_string(),
        Some(RunnerSandboxMode::Disabled) => "disabled".to_string(),
        None => default_config_value(),
    }
}

fn display_runner_plan_mode(value: Option<RunnerPlanMode>) -> String {
    match value {
        Some(RunnerPlanMode::Default) => "default".to_string(),
        Some(RunnerPlanMode::Enabled) => "enabled".to_string(),
        Some(RunnerPlanMode::Disabled) => "disabled".to_string(),
        None => default_config_value(),
    }
}

fn display_unsupported_option_policy(value: Option<UnsupportedOptionPolicy>) -> String {
    match value {
        Some(UnsupportedOptionPolicy::Ignore) => "ignore".to_string(),
        Some(UnsupportedOptionPolicy::Warn) => "warn".to_string(),
        Some(UnsupportedOptionPolicy::Error) => "error".to_string(),
        None => default_config_value(),
    }
}

fn display_git_revert_mode(value: Option<GitRevertMode>) -> String {
    match value {
        Some(GitRevertMode::Ask) => "ask".to_string(),
        Some(GitRevertMode::Enabled) => "enabled".to_string(),
        Some(GitRevertMode::Disabled) => "disabled".to_string(),
        None => default_config_value(),
    }
}

fn display_model(value: Option<&Model>) -> String {
    match value {
        Some(model) => model.as_str().to_string(),
        None => default_config_value(),
    }
}

fn display_string(value: Option<&String>) -> String {
    match value {
        Some(text) if !text.trim().is_empty() => text.to_string(),
        _ => default_config_value(),
    }
}

fn display_path(value: Option<&PathBuf>) -> String {
    match value {
        Some(path) => path.to_string_lossy().to_string(),
        None => default_config_value(),
    }
}

fn display_u8(value: Option<u8>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => default_config_value(),
    }
}

fn display_bool(value: Option<bool>) -> String {
    match value {
        Some(true) => "true".to_string(),
        Some(false) => "false".to_string(),
        None => default_config_value(),
    }
}

fn cycle_project_type(value: Option<ProjectType>) -> Option<ProjectType> {
    match value {
        None => Some(ProjectType::Code),
        Some(ProjectType::Code) => Some(ProjectType::Docs),
        Some(ProjectType::Docs) => None,
    }
}

fn cycle_runner(value: Option<Runner>) -> Option<Runner> {
    match value {
        None => Some(Runner::Codex),
        Some(Runner::Codex) => Some(Runner::Opencode),
        Some(Runner::Opencode) => Some(Runner::Gemini),
        Some(Runner::Gemini) => Some(Runner::Claude),
        Some(Runner::Claude) => Some(Runner::Cursor),
        Some(Runner::Cursor) => Some(Runner::Kimi),
        Some(Runner::Kimi) => Some(Runner::Pi),
        Some(Runner::Pi) => None,
    }
}

fn cycle_reasoning_effort(value: Option<ReasoningEffort>) -> Option<ReasoningEffort> {
    match value {
        None => Some(ReasoningEffort::Low),
        Some(ReasoningEffort::Low) => Some(ReasoningEffort::Medium),
        Some(ReasoningEffort::Medium) => Some(ReasoningEffort::High),
        Some(ReasoningEffort::High) => Some(ReasoningEffort::XHigh),
        Some(ReasoningEffort::XHigh) => None,
    }
}

fn cycle_claude_permission_mode(
    value: Option<ClaudePermissionMode>,
) -> Option<ClaudePermissionMode> {
    match value {
        None => Some(ClaudePermissionMode::AcceptEdits),
        Some(ClaudePermissionMode::AcceptEdits) => Some(ClaudePermissionMode::BypassPermissions),
        Some(ClaudePermissionMode::BypassPermissions) => None,
    }
}

fn cycle_runner_output_format(value: Option<RunnerOutputFormat>) -> Option<RunnerOutputFormat> {
    match value {
        None => Some(RunnerOutputFormat::StreamJson),
        Some(RunnerOutputFormat::StreamJson) => Some(RunnerOutputFormat::Json),
        Some(RunnerOutputFormat::Json) => Some(RunnerOutputFormat::Text),
        Some(RunnerOutputFormat::Text) => None,
    }
}

fn cycle_runner_verbosity(value: Option<RunnerVerbosity>) -> Option<RunnerVerbosity> {
    match value {
        None => Some(RunnerVerbosity::Quiet),
        Some(RunnerVerbosity::Quiet) => Some(RunnerVerbosity::Normal),
        Some(RunnerVerbosity::Normal) => Some(RunnerVerbosity::Verbose),
        Some(RunnerVerbosity::Verbose) => None,
    }
}

fn cycle_runner_approval_mode(value: Option<RunnerApprovalMode>) -> Option<RunnerApprovalMode> {
    match value {
        None => Some(RunnerApprovalMode::Default),
        Some(RunnerApprovalMode::Default) => Some(RunnerApprovalMode::AutoEdits),
        Some(RunnerApprovalMode::AutoEdits) => Some(RunnerApprovalMode::Yolo),
        Some(RunnerApprovalMode::Yolo) => Some(RunnerApprovalMode::Safe),
        Some(RunnerApprovalMode::Safe) => None,
    }
}

fn cycle_runner_sandbox_mode(value: Option<RunnerSandboxMode>) -> Option<RunnerSandboxMode> {
    match value {
        None => Some(RunnerSandboxMode::Default),
        Some(RunnerSandboxMode::Default) => Some(RunnerSandboxMode::Enabled),
        Some(RunnerSandboxMode::Enabled) => Some(RunnerSandboxMode::Disabled),
        Some(RunnerSandboxMode::Disabled) => None,
    }
}

fn cycle_runner_plan_mode(value: Option<RunnerPlanMode>) -> Option<RunnerPlanMode> {
    match value {
        None => Some(RunnerPlanMode::Default),
        Some(RunnerPlanMode::Default) => Some(RunnerPlanMode::Enabled),
        Some(RunnerPlanMode::Enabled) => Some(RunnerPlanMode::Disabled),
        Some(RunnerPlanMode::Disabled) => None,
    }
}

fn cycle_unsupported_option_policy(
    value: Option<UnsupportedOptionPolicy>,
) -> Option<UnsupportedOptionPolicy> {
    match value {
        None => Some(UnsupportedOptionPolicy::Ignore),
        Some(UnsupportedOptionPolicy::Ignore) => Some(UnsupportedOptionPolicy::Warn),
        Some(UnsupportedOptionPolicy::Warn) => Some(UnsupportedOptionPolicy::Error),
        Some(UnsupportedOptionPolicy::Error) => None,
    }
}

fn cycle_git_revert_mode(value: Option<GitRevertMode>) -> Option<GitRevertMode> {
    match value {
        None => Some(GitRevertMode::Ask),
        Some(GitRevertMode::Ask) => Some(GitRevertMode::Enabled),
        Some(GitRevertMode::Enabled) => Some(GitRevertMode::Disabled),
        Some(GitRevertMode::Disabled) => None,
    }
}

fn cycle_bool(value: Option<bool>) -> Option<bool> {
    match value {
        None => Some(true),
        Some(true) => Some(false),
        Some(false) => None,
    }
}

fn cycle_phases(value: Option<u8>) -> Option<u8> {
    match value {
        None => Some(1),
        Some(1) => Some(2),
        Some(2) => Some(3),
        Some(3) => None,
        Some(_) => None,
    }
}

fn runner_cli_defaults_mut(
    agent: &mut crate::contracts::AgentConfig,
) -> &mut RunnerCliOptionsPatch {
    if agent.runner_cli.is_none() {
        agent.runner_cli = Some(RunnerCliConfigRoot::default());
    }

    &mut agent.runner_cli.as_mut().expect("runner_cli root").defaults
}

fn runner_cli_patch_is_empty(patch: &RunnerCliOptionsPatch) -> bool {
    patch.output_format.is_none()
        && patch.verbosity.is_none()
        && patch.approval_mode.is_none()
        && patch.sandbox.is_none()
        && patch.plan_mode.is_none()
        && patch.unsupported_option_policy.is_none()
}

fn prune_runner_cli_root(agent: &mut crate::contracts::AgentConfig) {
    let Some(root) = agent.runner_cli.as_ref() else {
        return;
    };
    if root.runners.is_empty() && runner_cli_patch_is_empty(&root.defaults) {
        agent.runner_cli = None;
    }
}
