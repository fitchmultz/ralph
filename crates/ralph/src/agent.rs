//! Agent argument resolution and configuration.
//!
//! This module provides a unified API for resolving agent settings across
//! the Ralph codebase. It consolidates CLI argument parsing, override resolution,
//! and agent settings configuration.

use crate::config;
use crate::contracts::{GitRevertMode, Model, ReasoningEffort, Runner};
use anyhow::{anyhow, bail, Result};
use clap::Args;

/// CLI arguments for agent configuration.
///
/// Used by `task` and `scan` commands.
#[derive(Args, Clone, Debug, Default)]
pub struct AgentArgs {
    /// Runner override for this invocation (codex, opencode, gemini, claude).
    /// Overrides task.agent and config.
    #[arg(long)]
    pub runner: Option<String>,

    /// Model override for this invocation. Overrides task.agent and config.
    /// Allowed: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus
    /// (codex supports only gpt-5.2-codex/gpt-5.2; opencode/gemini/claude accept arbitrary model ids).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort override (minimal, low, medium, high).
    /// Ignored for other runners.
    #[arg(long)]
    pub effort: Option<String>,

    /// Force RepoPrompt required (must use context_builder).
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,
}

/// Extended agent arguments for run commands (includes phases).
#[derive(Args, Clone, Debug, Default)]
pub struct RunAgentArgs {
    /// Runner override for this invocation (codex, opencode, gemini, claude).
    /// Overrides task.agent and config.
    #[arg(long)]
    pub runner: Option<String>,

    /// Model override for this invocation. Overrides task.agent and config.
    /// Allowed: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus
    /// (codex supports only gpt-5.2-codex/gpt-5.2; opencode/gemini/claude accept arbitrary model ids).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort override (minimal, low, medium, high).
    /// Ignored for other runners.
    #[arg(long)]
    pub effort: Option<String>,

    /// Execution shape:
    /// - 1 => single-pass execution (no mandated planning step)
    /// - 2 => two-pass execution (plan then implement)
    /// - 3 => three-pass execution (plan, implement+CI, review+complete)
    ///
    /// If omitted, defaults to config `agent.phases`.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
    pub phases: Option<u8>,

    /// Force RepoPrompt required (must use context_builder).
    #[arg(long, conflicts_with = "rp_off")]
    pub rp_on: bool,

    /// Force RepoPrompt not required.
    #[arg(long, conflicts_with = "rp_on")]
    pub rp_off: bool,

    /// Git revert mode for automatic error handling (ask, enabled, disabled).
    #[arg(long, value_parser = ["ask", "enabled", "disabled"])]
    pub git_revert_mode: Option<String>,

    /// Include draft tasks when selecting what to run.
    #[arg(long)]
    pub include_draft: bool,
}

/// Agent overrides from CLI arguments.
///
/// These overrides take precedence over task.agent and config defaults.
#[derive(Debug, Clone, Default)]
pub struct AgentOverrides {
    pub runner: Option<Runner>,
    pub model: Option<Model>,
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Execution shape override:
    /// - 1 => single-pass execution
    /// - 2 => two-pass execution (plan then implement)
    /// - 3 => three-pass execution (plan, implement+CI, review+complete)
    pub phases: Option<u8>,
    pub repoprompt_required: Option<bool>,
    pub git_revert_mode: Option<GitRevertMode>,
    pub include_draft: Option<bool>,
}

/// Parse a runner string into a Runner enum.
pub fn parse_runner(value: &str) -> Result<Runner> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "codex" => Ok(Runner::Codex),
        "opencode" => Ok(Runner::Opencode),
        "gemini" => Ok(Runner::Gemini),
        "claude" => Ok(Runner::Claude),
        _ => bail!(
            "Invalid runner: --runner must be 'codex', 'opencode', 'gemini', or 'claude' (got: {}). Set a supported runner in .ralph/config.json or via the --runner flag.",
            value.trim()
        ),
    }
}

/// Parse git revert mode from a CLI string.
pub fn parse_git_revert_mode(value: &str) -> Result<GitRevertMode> {
    value.parse().map_err(|err: &str| anyhow!(err))
}

/// Resolve agent overrides from CLI arguments for run commands.
///
/// This parses the CLI arguments and validates runner/model compatibility.
pub fn resolve_run_agent_overrides(args: &RunAgentArgs) -> Result<AgentOverrides> {
    use crate::runner;

    let runner = match args.runner.as_deref() {
        Some(value) => Some(parse_runner(value)?),
        None => None,
    };

    let model = match args.model.as_deref() {
        Some(value) => Some(runner::parse_model(value)?),
        None => None,
    };

    let reasoning_effort = match args.effort.as_deref() {
        Some(value) => Some(runner::parse_reasoning_effort(value)?),
        None => None,
    };

    if let (Some(runner_kind), Some(model)) = (runner, model.as_ref()) {
        runner::validate_model_for_runner(runner_kind, model)?;
    }

    let repoprompt_required = if args.rp_on {
        Some(true)
    } else if args.rp_off {
        Some(false)
    } else {
        None
    };

    let git_revert_mode = match args.git_revert_mode.as_deref() {
        Some(value) => Some(parse_git_revert_mode(value)?),
        None => None,
    };

    let include_draft = if args.include_draft { Some(true) } else { None };

    Ok(AgentOverrides {
        runner,
        model,
        reasoning_effort,
        phases: args.phases,
        repoprompt_required,
        git_revert_mode,
        include_draft,
    })
}

/// Resolve agent overrides from CLI arguments for scan/task commands.
///
/// This is a simpler version that doesn't include phases.
pub fn resolve_agent_overrides(args: &AgentArgs) -> Result<AgentOverrides> {
    use crate::runner;

    let runner = match args.runner.as_deref() {
        Some(value) => Some(parse_runner(value)?),
        None => None,
    };

    let model = match args.model.as_deref() {
        Some(value) => Some(runner::parse_model(value)?),
        None => None,
    };

    let reasoning_effort = match args.effort.as_deref() {
        Some(value) => Some(runner::parse_reasoning_effort(value)?),
        None => None,
    };

    if let (Some(runner_kind), Some(model)) = (runner, model.as_ref()) {
        runner::validate_model_for_runner(runner_kind, model)?;
    }

    let repoprompt_required = if args.rp_on {
        Some(true)
    } else if args.rp_off {
        Some(false)
    } else {
        None
    };

    Ok(AgentOverrides {
        runner,
        model,
        reasoning_effort,
        phases: None,
        repoprompt_required,
        git_revert_mode: None,
        include_draft: None,
    })
}

/// Resolve whether RepoPrompt is required based on CLI flags and config.
pub fn resolve_rp_required(rp_on: bool, rp_off: bool, resolved: &config::Resolved) -> bool {
    if rp_on {
        return true;
    }
    if rp_off {
        return false;
    }
    resolved.config.agent.require_repoprompt.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{AgentConfig, ClaudePermissionMode, Config, GitRevertMode, QueueConfig};
    use tempfile::TempDir;

    fn resolved_with_defaults() -> config::Resolved {
        let dir = TempDir::new().expect("temp dir");
        let repo_root = dir.path().to_path_buf();

        let cfg = Config {
            agent: AgentConfig {
                runner: None,
                model: None,
                reasoning_effort: None,
                codex_bin: Some("codex".to_string()),
                opencode_bin: Some("opencode".to_string()),
                gemini_bin: Some("gemini".to_string()),
                claude_bin: Some("claude".to_string()),
                phases: Some(2),
                claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
                require_repoprompt: None,
                git_revert_mode: Some(GitRevertMode::Ask),
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
    fn parse_runner_accepts_valid_runners() {
        assert!(matches!(parse_runner("codex"), Ok(Runner::Codex)));
        assert!(matches!(parse_runner("opencode"), Ok(Runner::Opencode)));
        assert!(matches!(parse_runner("gemini"), Ok(Runner::Gemini)));
        assert!(matches!(parse_runner("claude"), Ok(Runner::Claude)));
        assert!(matches!(parse_runner("CODEX"), Ok(Runner::Codex)));
    }

    #[test]
    fn parse_runner_rejects_invalid_runners() {
        assert!(parse_runner("invalid").is_err());
        assert!(parse_runner("").is_err());
    }

    #[test]
    fn resolve_rp_required_cli_on_overrides_config() {
        let resolved = resolved_with_defaults();
        assert!(resolve_rp_required(true, false, &resolved));
    }

    #[test]
    fn resolve_rp_required_cli_off_overrides_config() {
        let resolved = resolved_with_defaults();
        assert!(!resolve_rp_required(false, true, &resolved));
    }

    #[test]
    fn resolve_rp_required_uses_config_when_cli_not_set() {
        let mut resolved = resolved_with_defaults();
        resolved.config.agent.require_repoprompt = Some(true);
        assert!(resolve_rp_required(false, false, &resolved));

        resolved.config.agent.require_repoprompt = Some(false);
        assert!(!resolve_rp_required(false, false, &resolved));
    }

    #[test]
    fn resolve_agent_overrides_parses_valid_args() {
        let args = AgentArgs {
            runner: Some("opencode".to_string()),
            model: Some("gpt-5.2".to_string()),
            effort: None,
            rp_on: false,
            rp_off: false,
        };

        let overrides = resolve_agent_overrides(&args).unwrap();
        assert_eq!(overrides.runner, Some(Runner::Opencode));
        assert_eq!(overrides.model, Some(Model::Gpt52));
        assert_eq!(overrides.reasoning_effort, None);
        assert_eq!(overrides.repoprompt_required, None);
        assert_eq!(overrides.git_revert_mode, None);
        assert_eq!(overrides.include_draft, None);
    }

    #[test]
    fn resolve_agent_overrides_sets_rp_flags() {
        let args = AgentArgs {
            runner: None,
            model: None,
            effort: None,
            rp_on: true,
            rp_off: false,
        };

        let overrides = resolve_agent_overrides(&args).unwrap();
        assert_eq!(overrides.repoprompt_required, Some(true));
        assert_eq!(overrides.git_revert_mode, None);
        assert_eq!(overrides.include_draft, None);
    }

    #[test]
    fn resolve_run_agent_overrides_includes_phases() {
        let args = RunAgentArgs {
            runner: Some("codex".to_string()),
            model: Some("gpt-5.2-codex".to_string()),
            effort: Some("high".to_string()),
            phases: Some(2),
            rp_on: false,
            rp_off: false,
            git_revert_mode: Some("enabled".to_string()),
            include_draft: true,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();
        assert_eq!(overrides.runner, Some(Runner::Codex));
        assert_eq!(overrides.model, Some(Model::Gpt52Codex));
        assert_eq!(overrides.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(overrides.phases, Some(2));
        assert_eq!(overrides.git_revert_mode, Some(GitRevertMode::Enabled));
        assert_eq!(overrides.include_draft, Some(true));
    }
}
