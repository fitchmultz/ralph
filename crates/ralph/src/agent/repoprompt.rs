//! RepoPrompt mode and flag resolution.
//!
//! Responsibilities:
//! - Define RepoPromptMode enum with clap ValueEnum support.
//! - Define RepopromptFlags struct for resolved flag state.
//! - Resolve RepoPrompt flags from CLI mode, config, and overrides.
//!
//! Not handled here:
//! - CLI argument struct definitions (see `super::args`).
//! - Override resolution logic (see `super::resolve`).
//! - Runner/model parsing (see `crate::runner`).
//!
//! Invariants/assumptions:
//! - RepoPromptMode::Tools enables tool injection without plan requirement.
//! - RepoPromptMode::Plan enables both tool injection and plan requirement.
//! - RepoPromptMode::Off disables both features.
//! - Config values are used as fallback when CLI mode is not specified.

use crate::config;
use crate::contracts::AgentConfig;
use clap::ValueEnum;

/// RepoPrompt mode selection from CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum RepoPromptMode {
    #[value(name = "tools")]
    Tools,
    #[value(name = "plan")]
    Plan,
    #[value(name = "off")]
    Off,
}

/// Resolved RepoPrompt flags after processing mode/config/overrides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepopromptFlags {
    pub plan_required: bool,
    pub tool_injection: bool,
}

/// Convert a RepoPromptMode to its corresponding flags.
pub(crate) fn repoprompt_flags_from_mode(mode: RepoPromptMode) -> RepopromptFlags {
    match mode {
        RepoPromptMode::Tools => RepopromptFlags {
            plan_required: false,
            tool_injection: true,
        },
        RepoPromptMode::Plan => RepopromptFlags {
            plan_required: true,
            tool_injection: true,
        },
        RepoPromptMode::Off => RepopromptFlags {
            plan_required: false,
            tool_injection: false,
        },
    }
}

/// Resolve RepoPrompt flags from agent config defaults.
pub(crate) fn resolve_repoprompt_flags_from_agent_config(agent: &AgentConfig) -> RepopromptFlags {
    let plan_required = agent.repoprompt_plan_required.unwrap_or(false);
    let tool_injection = agent.repoprompt_tool_injection.unwrap_or(false);
    RepopromptFlags {
        plan_required,
        tool_injection,
    }
}

/// Resolve RepoPrompt flags from CLI mode or config defaults.
pub fn resolve_repoprompt_flags(
    repo_prompt: Option<RepoPromptMode>,
    resolved: &config::Resolved,
) -> RepopromptFlags {
    if let Some(mode) = repo_prompt {
        return repoprompt_flags_from_mode(mode);
    }
    resolve_repoprompt_flags_from_agent_config(&resolved.config.agent)
}

/// Resolve whether RepoPrompt tooling reminder injection is required.
pub fn resolve_rp_required(
    repo_prompt: Option<RepoPromptMode>,
    resolved: &config::Resolved,
) -> bool {
    resolve_repoprompt_flags(repo_prompt, resolved).tool_injection
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        AgentConfig, ClaudePermissionMode, Config, GitRevertMode, NotificationConfig, QueueConfig,
        RunnerRetryConfig,
    };
    use tempfile::TempDir;

    fn resolved_with_defaults() -> config::Resolved {
        let dir = TempDir::new().expect("temp dir");
        let repo_root = dir.path().to_path_buf();

        let cfg = Config {
            agent: AgentConfig {
                runner: None,
                model: None,
                reasoning_effort: None,
                iterations: None,
                followup_reasoning_effort: None,
                codex_bin: Some("codex".to_string()),
                opencode_bin: Some("opencode".to_string()),
                gemini_bin: Some("gemini".to_string()),
                claude_bin: Some("claude".to_string()),
                cursor_bin: Some("agent".to_string()),
                kimi_bin: Some("kimi".to_string()),
                pi_bin: Some("pi".to_string()),
                phases: Some(2),
                update_task_before_run: None,
                fail_on_prerun_update_error: None,
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
                runner_cli: None,
                phase_overrides: None,
                instruction_files: None,
                repoprompt_plan_required: None,
                repoprompt_tool_injection: None,
                ci_gate_command: Some("make ci".to_string()),
                ci_gate_enabled: Some(true),
                git_revert_mode: Some(GitRevertMode::Ask),
                git_commit_push_enabled: Some(true),
                notification: NotificationConfig::default(),
                webhook: crate::contracts::WebhookConfig::default(),
                runner_retry: RunnerRetryConfig::default(),
                session_timeout_hours: None,
                scan_prompt_version: None,
            },
            queue: QueueConfig::default(),
            ..Config::default()
        };

        config::Resolved {
            config: cfg,
            repo_root: repo_root.clone(),
            queue_path: repo_root.join(".ralph/queue.json"),
            done_path: repo_root.join(".ralph/done.json"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.json")),
        }
    }

    #[test]
    fn resolve_rp_required_cli_plan_overrides_config() {
        let resolved = resolved_with_defaults();
        assert!(resolve_rp_required(Some(RepoPromptMode::Plan), &resolved));
    }

    #[test]
    fn resolve_rp_required_cli_off_overrides_config() {
        let resolved = resolved_with_defaults();
        assert!(!resolve_rp_required(Some(RepoPromptMode::Off), &resolved));
    }

    #[test]
    fn resolve_rp_required_uses_config_when_cli_not_set() {
        let mut resolved = resolved_with_defaults();
        resolved.config.agent.repoprompt_tool_injection = Some(true);
        assert!(resolve_rp_required(None, &resolved));

        resolved.config.agent.repoprompt_tool_injection = Some(false);
        assert!(!resolve_rp_required(None, &resolved));
    }

    #[test]
    fn resolve_repoprompt_flags_defaults_false_when_unset() {
        let resolved = resolved_with_defaults();
        let flags = resolve_repoprompt_flags(None, &resolved);
        assert!(!flags.plan_required);
        assert!(!flags.tool_injection);
    }

    #[test]
    fn resolve_repoprompt_flags_uses_config_fields() {
        let mut resolved = resolved_with_defaults();
        resolved.config.agent.repoprompt_plan_required = Some(true);
        resolved.config.agent.repoprompt_tool_injection = Some(false);

        let flags = resolve_repoprompt_flags(None, &resolved);
        assert!(flags.plan_required);
        assert!(!flags.tool_injection);
    }
}
