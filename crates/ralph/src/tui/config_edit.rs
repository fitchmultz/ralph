use super::app::App;
use crate::contracts::{
    ClaudePermissionMode, GitRevertMode, Model, ProjectType, ReasoningEffort, Runner,
};
use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFieldKind {
    Cycle,
    Toggle,
    Text,
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
}

impl App {
    pub(crate) fn config_entries(&self) -> Vec<ConfigEntry> {
        vec![
            ConfigEntry {
                key: ConfigKey::ProjectType,
                label: "project_type",
                value: display_project_type(self.project_config.project_type),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::QueueFile,
                label: "queue.file",
                value: display_path(self.project_config.queue.file.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::QueueDoneFile,
                label: "queue.done_file",
                value: display_path(self.project_config.queue.done_file.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::QueueIdPrefix,
                label: "queue.id_prefix",
                value: display_string(self.project_config.queue.id_prefix.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::QueueIdWidth,
                label: "queue.id_width",
                value: display_u8(self.project_config.queue.id_width),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentRunner,
                label: "agent.runner",
                value: display_runner(self.project_config.agent.runner),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentModel,
                label: "agent.model",
                value: display_model(self.project_config.agent.model.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentReasoningEffort,
                label: "agent.reasoning_effort",
                value: display_reasoning_effort(self.project_config.agent.reasoning_effort),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentIterations,
                label: "agent.iterations",
                value: display_u8(self.project_config.agent.iterations),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentFollowupReasoningEffort,
                label: "agent.followup_reasoning_effort",
                value: display_reasoning_effort(
                    self.project_config.agent.followup_reasoning_effort,
                ),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentCodexBin,
                label: "agent.codex_bin",
                value: display_string(self.project_config.agent.codex_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentOpencodeBin,
                label: "agent.opencode_bin",
                value: display_string(self.project_config.agent.opencode_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentGeminiBin,
                label: "agent.gemini_bin",
                value: display_string(self.project_config.agent.gemini_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentClaudeBin,
                label: "agent.claude_bin",
                value: display_string(self.project_config.agent.claude_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentCursorBin,
                label: "agent.cursor_bin",
                value: display_string(self.project_config.agent.cursor_bin.as_ref()),
                kind: ConfigFieldKind::Text,
            },
            ConfigEntry {
                key: ConfigKey::AgentClaudePermissionMode,
                label: "agent.claude_permission_mode",
                value: display_claude_permission_mode(
                    self.project_config.agent.claude_permission_mode,
                ),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentRepopromptPlanRequired,
                label: "agent.repoprompt_plan_required",
                value: display_bool(self.project_config.agent.repoprompt_plan_required),
                kind: ConfigFieldKind::Toggle,
            },
            ConfigEntry {
                key: ConfigKey::AgentRepopromptToolInjection,
                label: "agent.repoprompt_tool_injection",
                value: display_bool(self.project_config.agent.repoprompt_tool_injection),
                kind: ConfigFieldKind::Toggle,
            },
            ConfigEntry {
                key: ConfigKey::AgentGitRevertMode,
                label: "agent.git_revert_mode",
                value: display_git_revert_mode(self.project_config.agent.git_revert_mode),
                kind: ConfigFieldKind::Cycle,
            },
            ConfigEntry {
                key: ConfigKey::AgentGitCommitPushEnabled,
                label: "agent.git_commit_push_enabled",
                value: display_bool(self.project_config.agent.git_commit_push_enabled),
                kind: ConfigFieldKind::Toggle,
            },
            ConfigEntry {
                key: ConfigKey::AgentPhases,
                label: "agent.phases",
                value: display_u8(self.project_config.agent.phases),
                kind: ConfigFieldKind::Cycle,
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
        Some(Runner::Cursor) => None,
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
