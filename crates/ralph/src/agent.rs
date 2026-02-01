//! Agent argument resolution and configuration.
//!
//! Responsibilities:
//! - Parse and validate agent-related CLI inputs into structured overrides.
//! - Resolve RepoPrompt requirements from CLI selections and config defaults.
//! - Provide helpers for runner/model/effort parsing shared across commands.
//!
//! Not handled here:
//! - CLI definitions outside agent-related flags.
//! - Runner execution, prompt rendering, or queue mutations.
//! - Persisting configuration changes.
//!
//! Invariants/assumptions:
//! - Callers pass normalized CLI arguments (including any short-flag aliases).
//! - RepoPrompt mode selections map to both plan/tool flags consistently.

use crate::config;
use crate::contracts::{
    AgentConfig, GitRevertMode, Model, PhaseOverrideConfig, PhaseOverrides, ReasoningEffort,
    Runner, RunnerApprovalMode, RunnerCliOptionsPatch, RunnerOutputFormat, RunnerPlanMode,
    RunnerSandboxMode, RunnerVerbosity, UnsupportedOptionPolicy,
};
use anyhow::{Result, anyhow, bail};
use clap::{Args, ValueEnum};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum RepoPromptMode {
    #[value(name = "tools")]
    Tools,
    #[value(name = "plan")]
    Plan,
    #[value(name = "off")]
    Off,
}

/// CLI arguments for runner CLI overrides.
#[derive(Args, Clone, Debug, Default)]
pub struct RunnerCliArgs {
    /// Desired runner output format (stream-json, json, text). Ralph execution requires stream-json.
    #[arg(long)]
    pub output_format: Option<String>,

    /// Desired verbosity (quiet, normal, verbose). Only some runners support this.
    #[arg(long)]
    pub verbosity: Option<String>,

    /// Desired approval mode (default, auto-edits, yolo, safe). Default is yolo.
    #[arg(long)]
    pub approval_mode: Option<String>,

    /// Desired sandbox mode (default, enabled, disabled). Only some runners support this.
    #[arg(long)]
    pub sandbox: Option<String>,

    /// Desired plan/read-only mode (default, enabled, disabled). Only Cursor currently supports this.
    #[arg(long)]
    pub plan_mode: Option<String>,

    /// Policy for unsupported options (ignore, warn, error). Default is warn.
    #[arg(long)]
    pub unsupported_option_policy: Option<String>,
}

/// CLI arguments for agent configuration.
///
/// Used by `task` and `scan` commands.
#[derive(Args, Clone, Debug, Default)]
pub struct AgentArgs {
    /// Runner override for this invocation (codex, opencode, gemini, claude, cursor).
    /// Overrides task.agent and config.
    #[arg(long)]
    pub runner: Option<String>,

    /// Model override for this invocation. Overrides task.agent and config.
    /// Allowed: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus
    /// (codex supports only gpt-5.2-codex/gpt-5.2; opencode/gemini/claude/cursor accept arbitrary model ids).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort override (low, medium, high, xhigh).
    /// Ignored for other runners.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<RepoPromptMode>,

    #[command(flatten)]
    pub runner_cli: RunnerCliArgs,
}

/// Extended agent arguments for run commands (includes phases).
#[derive(Args, Clone, Debug, Default)]
pub struct RunAgentArgs {
    /// Runner override for this invocation (codex, opencode, gemini, claude, cursor).
    /// Overrides task.agent and config.
    #[arg(long)]
    pub runner: Option<String>,

    /// Model override for this invocation. Overrides task.agent and config.
    /// Allowed: gpt-5.2-codex, gpt-5.2, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus
    /// (codex supports only gpt-5.2-codex/gpt-5.2; opencode/gemini/claude/cursor accept arbitrary model ids).
    #[arg(long)]
    pub model: Option<String>,

    /// Codex reasoning effort override (low, medium, high, xhigh).
    /// Ignored for other runners.
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    #[command(flatten)]
    pub runner_cli: RunnerCliArgs,

    /// Execution shape:
    /// - 1 => single-pass execution (no mandated planning step)
    /// - 2 => two-pass execution (plan then implement)
    /// - 3 => three-pass execution (plan, implement+CI, review+complete)
    ///
    /// If omitted, defaults to config `agent.phases`.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
    pub phases: Option<u8>,

    /// Quick mode: skip planning phase and run single-pass execution.
    ///
    /// Equivalent to --phases=1. Cannot be used with --phases.
    #[arg(long, conflicts_with = "phases")]
    pub quick: bool,

    /// RepoPrompt mode (tools, plan, off). Alias: -rp.
    #[arg(long = "repo-prompt", value_enum, value_name = "MODE")]
    pub repo_prompt: Option<RepoPromptMode>,

    /// Git revert mode for automatic error handling (ask, enabled, disabled).
    #[arg(long, value_parser = ["ask", "enabled", "disabled"])]
    pub git_revert_mode: Option<String>,

    /// Enable automatic git commit and push after successful runs.
    #[arg(long, conflicts_with = "git_commit_push_off")]
    pub git_commit_push_on: bool,

    /// Disable automatic git commit and push after successful runs.
    #[arg(long, conflicts_with = "git_commit_push_on")]
    pub git_commit_push_off: bool,

    /// Include draft tasks when selecting what to run.
    #[arg(long)]
    pub include_draft: bool,

    /// Automatically update the selected task immediately before running it.
    ///
    /// This runs the equivalent of: `ralph task update <TASK_ID>` once per task.
    #[arg(long, conflicts_with = "no_update_task")]
    pub update_task: bool,

    /// Disable automatic pre-run task update (overrides config).
    #[arg(long, conflicts_with = "update_task")]
    pub no_update_task: bool,

    /// Enable desktop notification on task completion (overrides config).
    #[arg(long, conflicts_with = "no_notify")]
    pub notify: bool,

    /// Disable desktop notification on task completion (overrides config).
    #[arg(long, conflicts_with = "notify")]
    pub no_notify: bool,

    /// Enable desktop notification on task failure (overrides config).
    #[arg(long, conflicts_with = "no_notify_fail")]
    pub notify_fail: bool,

    /// Disable desktop notification on task failure (overrides config).
    #[arg(long, conflicts_with = "notify_fail")]
    pub no_notify_fail: bool,

    /// Enable sound alert with notification (requires --notify or config enabled).
    #[arg(long)]
    pub notify_sound: bool,

    /// Enable strict LFS validation before commit (fail if filters misconfigured).
    #[arg(long)]
    pub lfs_check: bool,

    /// Disable progress indicators and celebrations.
    #[arg(long)]
    pub no_progress: bool,

    // Phase 1 overrides
    /// Runner override for Phase 1 (planning).
    #[arg(long, value_name = "RUNNER")]
    pub runner_phase1: Option<String>,

    /// Model override for Phase 1 (planning).
    #[arg(long, value_name = "MODEL")]
    pub model_phase1: Option<String>,

    /// Reasoning effort override for Phase 1 (planning).
    #[arg(long, value_name = "EFFORT")]
    pub effort_phase1: Option<String>,

    // Phase 2 overrides
    /// Runner override for Phase 2 (implementation).
    #[arg(long, value_name = "RUNNER")]
    pub runner_phase2: Option<String>,

    /// Model override for Phase 2 (implementation).
    #[arg(long, value_name = "MODEL")]
    pub model_phase2: Option<String>,

    /// Reasoning effort override for Phase 2 (implementation).
    #[arg(long, value_name = "EFFORT")]
    pub effort_phase2: Option<String>,

    // Phase 3 overrides
    /// Runner override for Phase 3 (review).
    #[arg(long, value_name = "RUNNER")]
    pub runner_phase3: Option<String>,

    /// Model override for Phase 3 (review).
    #[arg(long, value_name = "MODEL")]
    pub model_phase3: Option<String>,

    /// Reasoning effort override for Phase 3 (review).
    #[arg(long, value_name = "EFFORT")]
    pub effort_phase3: Option<String>,
}

/// Agent overrides from CLI arguments.
///
/// These overrides take precedence over task.agent and config defaults.
#[derive(Debug, Clone, Default)]
pub struct AgentOverrides {
    pub runner: Option<Runner>,
    pub model: Option<Model>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: RunnerCliOptionsPatch,
    /// Execution shape override:
    /// - 1 => single-pass execution
    /// - 2 => two-pass execution (plan then implement)
    /// - 3 => three-pass execution (plan, implement+CI, review+complete)
    pub phases: Option<u8>,
    pub update_task_before_run: Option<bool>,
    pub fail_on_prerun_update_error: Option<bool>,
    pub repoprompt_plan_required: Option<bool>,
    pub repoprompt_tool_injection: Option<bool>,
    pub git_revert_mode: Option<GitRevertMode>,
    pub git_commit_push_enabled: Option<bool>,
    pub include_draft: Option<bool>,
    /// Enable/disable desktop notification on task completion.
    pub notify_on_complete: Option<bool>,
    /// Enable/disable desktop notification on task failure.
    pub notify_on_fail: Option<bool>,
    /// Enable/disable desktop notification when loop completes.
    pub notify_on_loop_complete: Option<bool>,
    /// Enable sound alert with notification.
    pub notify_sound: Option<bool>,
    /// Enable strict LFS validation before commit.
    pub lfs_check: Option<bool>,
    /// Disable progress indicators and celebrations.
    pub no_progress: Option<bool>,
    /// Per-phase overrides from CLI (phase1, phase2, phase3).
    pub phase_overrides: Option<PhaseOverrides>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepopromptFlags {
    pub plan_required: bool,
    pub tool_injection: bool,
}

fn repoprompt_flags_from_mode(mode: RepoPromptMode) -> RepopromptFlags {
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

fn parse_runner_cli_patch(args: &RunnerCliArgs) -> Result<RunnerCliOptionsPatch> {
    let output_format = match args.output_format.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerOutputFormat>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let verbosity = match args.verbosity.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerVerbosity>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let approval_mode = match args.approval_mode.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerApprovalMode>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let sandbox = match args.sandbox.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerSandboxMode>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let plan_mode = match args.plan_mode.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerPlanMode>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let unsupported_option_policy = match args.unsupported_option_policy.as_deref() {
        Some(value) => Some(
            value
                .parse::<UnsupportedOptionPolicy>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };

    Ok(RunnerCliOptionsPatch {
        output_format,
        verbosity,
        approval_mode,
        sandbox,
        plan_mode,
        unsupported_option_policy,
    })
}

/// Parse a runner string into a Runner enum.
pub fn parse_runner(value: &str) -> Result<Runner> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "codex" => Ok(Runner::Codex),
        "opencode" => Ok(Runner::Opencode),
        "gemini" => Ok(Runner::Gemini),
        "claude" => Ok(Runner::Claude),
        "cursor" => Ok(Runner::Cursor),
        "kimi" => Ok(Runner::Kimi),
        "pi" => Ok(Runner::Pi),
        _ => bail!(
            "Invalid runner: --runner must be 'codex', 'opencode', 'gemini', 'claude', 'cursor', 'kimi', or 'pi' (got: {}). Set a supported runner in .ralph/config.json or via the --runner flag.",
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
    let runner_cli = parse_runner_cli_patch(&args.runner_cli)?;

    if let (Some(runner_kind), Some(model)) = (runner, model.as_ref()) {
        runner::validate_model_for_runner(runner_kind, model)?;
    }

    let repoprompt_override = args.repo_prompt.map(repoprompt_flags_from_mode);

    let git_revert_mode = match args.git_revert_mode.as_deref() {
        Some(value) => Some(parse_git_revert_mode(value)?),
        None => None,
    };

    let git_commit_push_enabled = if args.git_commit_push_on {
        Some(true)
    } else if args.git_commit_push_off {
        Some(false)
    } else {
        None
    };

    let include_draft = if args.include_draft { Some(true) } else { None };

    let update_task_before_run = if args.update_task {
        Some(true)
    } else if args.no_update_task {
        Some(false)
    } else {
        None
    };

    // Handle --quick flag: when set, override phases to 1 (single-pass execution)
    let phases = if args.quick { Some(1) } else { args.phases };

    // Handle notification flags
    let notify_on_complete = if args.notify {
        Some(true)
    } else if args.no_notify {
        Some(false)
    } else {
        None
    };

    let notify_on_fail = if args.notify_fail {
        Some(true)
    } else if args.no_notify_fail {
        Some(false)
    } else {
        None
    };

    let notify_sound = if args.notify_sound { Some(true) } else { None };
    let lfs_check = if args.lfs_check { Some(true) } else { None };
    let no_progress = if args.no_progress { Some(true) } else { None };

    // Parse phase-specific overrides
    let mut phase_overrides = PhaseOverrides::default();

    // Phase 1
    if args.runner_phase1.is_some() || args.model_phase1.is_some() || args.effort_phase1.is_some() {
        phase_overrides.phase1 = Some(PhaseOverrideConfig {
            runner: args
                .runner_phase1
                .as_deref()
                .map(parse_runner)
                .transpose()?,
            model: args
                .model_phase1
                .as_deref()
                .map(runner::parse_model)
                .transpose()?,
            reasoning_effort: args
                .effort_phase1
                .as_deref()
                .map(runner::parse_reasoning_effort)
                .transpose()?,
        });
    }

    // Phase 2
    if args.runner_phase2.is_some() || args.model_phase2.is_some() || args.effort_phase2.is_some() {
        phase_overrides.phase2 = Some(PhaseOverrideConfig {
            runner: args
                .runner_phase2
                .as_deref()
                .map(parse_runner)
                .transpose()?,
            model: args
                .model_phase2
                .as_deref()
                .map(runner::parse_model)
                .transpose()?,
            reasoning_effort: args
                .effort_phase2
                .as_deref()
                .map(runner::parse_reasoning_effort)
                .transpose()?,
        });
    }

    // Phase 3
    if args.runner_phase3.is_some() || args.model_phase3.is_some() || args.effort_phase3.is_some() {
        phase_overrides.phase3 = Some(PhaseOverrideConfig {
            runner: args
                .runner_phase3
                .as_deref()
                .map(parse_runner)
                .transpose()?,
            model: args
                .model_phase3
                .as_deref()
                .map(runner::parse_model)
                .transpose()?,
            reasoning_effort: args
                .effort_phase3
                .as_deref()
                .map(runner::parse_reasoning_effort)
                .transpose()?,
        });
    }

    // Only set phase_overrides if any phase has overrides
    let phase_overrides = if phase_overrides.phase1.is_some()
        || phase_overrides.phase2.is_some()
        || phase_overrides.phase3.is_some()
    {
        Some(phase_overrides)
    } else {
        None
    };

    Ok(AgentOverrides {
        runner,
        model,
        reasoning_effort,
        runner_cli,
        phases,
        update_task_before_run,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: repoprompt_override.map(|flags| flags.plan_required),
        repoprompt_tool_injection: repoprompt_override.map(|flags| flags.tool_injection),
        git_revert_mode,
        git_commit_push_enabled,
        include_draft,
        notify_on_complete,
        notify_on_fail,
        notify_on_loop_complete: None,
        notify_sound,
        lfs_check,
        no_progress,
        phase_overrides,
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

    let repoprompt_override = args.repo_prompt.map(repoprompt_flags_from_mode);
    let runner_cli = parse_runner_cli_patch(&args.runner_cli)?;

    Ok(AgentOverrides {
        runner,
        model,
        reasoning_effort,
        runner_cli,
        phases: None,
        update_task_before_run: None,
        fail_on_prerun_update_error: None,
        repoprompt_plan_required: repoprompt_override.map(|flags| flags.plan_required),
        repoprompt_tool_injection: repoprompt_override.map(|flags| flags.tool_injection),
        git_revert_mode: None,
        git_commit_push_enabled: None,
        include_draft: None,
        notify_on_complete: None,
        notify_on_fail: None,
        notify_on_loop_complete: None,
        notify_sound: None,
        lfs_check: None,
        no_progress: None,
        phase_overrides: None,
    })
}

fn resolve_repoprompt_flags_from_agent_config(agent: &AgentConfig) -> RepopromptFlags {
    let plan_required = agent.repoprompt_plan_required.unwrap_or(false);
    let tool_injection = agent.repoprompt_tool_injection.unwrap_or(false);
    RepopromptFlags {
        plan_required,
        tool_injection,
    }
}

pub fn resolve_repoprompt_flags(
    repo_prompt: Option<RepoPromptMode>,
    resolved: &config::Resolved,
) -> RepopromptFlags {
    if let Some(mode) = repo_prompt {
        return repoprompt_flags_from_mode(mode);
    }
    resolve_repoprompt_flags_from_agent_config(&resolved.config.agent)
}

pub fn resolve_repoprompt_flags_from_overrides(
    overrides: &AgentOverrides,
    resolved: &config::Resolved,
) -> RepopromptFlags {
    let config_flags = resolve_repoprompt_flags_from_agent_config(&resolved.config.agent);
    let plan_required = overrides
        .repoprompt_plan_required
        .unwrap_or(config_flags.plan_required);
    let tool_injection = overrides
        .repoprompt_tool_injection
        .unwrap_or(config_flags.tool_injection);
    RepopromptFlags {
        plan_required,
        tool_injection,
    }
}

/// Resolve whether RepoPrompt tooling reminder injection is required based on CLI flags and config.
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
        RunnerApprovalMode, RunnerPlanMode, RunnerSandboxMode,
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
                session_timeout_hours: None,
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
        assert!(matches!(parse_runner("cursor"), Ok(Runner::Cursor)));
        assert!(matches!(parse_runner("kimi"), Ok(Runner::Kimi)));
        assert!(matches!(parse_runner("pi"), Ok(Runner::Pi)));
        assert!(matches!(parse_runner("CODEX"), Ok(Runner::Codex)));
    }

    #[test]
    fn parse_runner_rejects_invalid_runners() {
        assert!(parse_runner("invalid").is_err());
        assert!(parse_runner("").is_err());
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

    #[test]
    fn resolve_repoprompt_flags_from_overrides_take_precedence() {
        let resolved = resolved_with_defaults();
        let overrides = AgentOverrides {
            runner: None,
            model: None,
            reasoning_effort: None,
            runner_cli: RunnerCliOptionsPatch::default(),
            phases: None,
            update_task_before_run: None,
            fail_on_prerun_update_error: None,
            repoprompt_plan_required: Some(false),
            repoprompt_tool_injection: Some(true),
            git_revert_mode: None,
            git_commit_push_enabled: None,
            include_draft: None,
            notify_on_complete: None,
            notify_on_fail: None,
            notify_on_loop_complete: None,
            notify_sound: None,
            lfs_check: None,
            no_progress: None,
            phase_overrides: None,
        };

        let flags = resolve_repoprompt_flags_from_overrides(&overrides, &resolved);
        assert!(!flags.plan_required);
        assert!(flags.tool_injection);
    }

    #[test]
    fn resolve_agent_overrides_parses_valid_args() {
        let args = AgentArgs {
            runner: Some("opencode".to_string()),
            model: Some("gpt-5.2".to_string()),
            effort: None,
            repo_prompt: None,
            runner_cli: RunnerCliArgs::default(),
        };

        let overrides = resolve_agent_overrides(&args).unwrap();
        assert_eq!(overrides.runner, Some(Runner::Opencode));
        assert_eq!(overrides.model, Some(Model::Gpt52));
        assert_eq!(overrides.reasoning_effort, None);
        assert_eq!(overrides.repoprompt_plan_required, None);
        assert_eq!(overrides.repoprompt_tool_injection, None);
        assert_eq!(overrides.git_revert_mode, None);
        assert_eq!(overrides.git_commit_push_enabled, None);
        assert_eq!(overrides.include_draft, None);
    }

    #[test]
    fn resolve_agent_overrides_sets_rp_flags() {
        let args = AgentArgs {
            runner: None,
            model: None,
            effort: None,
            repo_prompt: Some(RepoPromptMode::Plan),
            runner_cli: RunnerCliArgs::default(),
        };

        let overrides = resolve_agent_overrides(&args).unwrap();
        assert_eq!(overrides.repoprompt_plan_required, Some(true));
        assert_eq!(overrides.repoprompt_tool_injection, Some(true));
        assert_eq!(overrides.git_revert_mode, None);
        assert_eq!(overrides.git_commit_push_enabled, None);
        assert_eq!(overrides.include_draft, None);
    }

    #[test]
    fn resolve_agent_overrides_maps_repo_prompt_modes() {
        let tools_args = AgentArgs {
            runner: None,
            model: None,
            effort: None,
            repo_prompt: Some(RepoPromptMode::Tools),
            runner_cli: RunnerCliArgs::default(),
        };
        let tools_overrides = resolve_agent_overrides(&tools_args).unwrap();
        assert_eq!(tools_overrides.repoprompt_plan_required, Some(false));
        assert_eq!(tools_overrides.repoprompt_tool_injection, Some(true));

        let off_args = AgentArgs {
            runner: None,
            model: None,
            effort: None,
            repo_prompt: Some(RepoPromptMode::Off),
            runner_cli: RunnerCliArgs::default(),
        };
        let off_overrides = resolve_agent_overrides(&off_args).unwrap();
        assert_eq!(off_overrides.repoprompt_plan_required, Some(false));
        assert_eq!(off_overrides.repoprompt_tool_injection, Some(false));
    }

    #[test]
    fn resolve_agent_overrides_parses_runner_cli_args() {
        let args = AgentArgs {
            runner: None,
            model: None,
            effort: None,
            repo_prompt: None,
            runner_cli: RunnerCliArgs {
                approval_mode: Some("auto-edits".to_string()),
                sandbox: Some("disabled".to_string()),
                ..RunnerCliArgs::default()
            },
        };

        let overrides = resolve_agent_overrides(&args).unwrap();
        assert_eq!(
            overrides.runner_cli.approval_mode,
            Some(RunnerApprovalMode::AutoEdits)
        );
        assert_eq!(
            overrides.runner_cli.sandbox,
            Some(RunnerSandboxMode::Disabled)
        );
    }

    #[test]
    fn resolve_run_agent_overrides_includes_phases() {
        let args = RunAgentArgs {
            runner: Some("codex".to_string()),
            model: Some("gpt-5.2-codex".to_string()),
            effort: Some("high".to_string()),
            runner_cli: RunnerCliArgs::default(),
            phases: Some(2),
            quick: false,
            repo_prompt: None,
            git_revert_mode: Some("enabled".to_string()),
            git_commit_push_on: false,
            git_commit_push_off: true,
            include_draft: true,
            update_task: true,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: None,
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();
        assert_eq!(overrides.runner, Some(Runner::Codex));
        assert_eq!(overrides.model, Some(Model::Gpt52Codex));
        assert_eq!(overrides.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(overrides.phases, Some(2));
        assert_eq!(overrides.update_task_before_run, Some(true));
        assert_eq!(overrides.git_revert_mode, Some(GitRevertMode::Enabled));
        assert_eq!(overrides.git_commit_push_enabled, Some(false));
        assert_eq!(overrides.include_draft, Some(true));
    }

    #[test]
    fn resolve_run_agent_overrides_parses_runner_cli_args() {
        let args = RunAgentArgs {
            runner: None,
            model: None,
            effort: None,
            runner_cli: RunnerCliArgs {
                approval_mode: Some("yolo".to_string()),
                plan_mode: Some("enabled".to_string()),
                ..RunnerCliArgs::default()
            },
            phases: None,
            quick: false,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: None,
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();
        assert_eq!(
            overrides.runner_cli.approval_mode,
            Some(RunnerApprovalMode::Yolo)
        );
        assert_eq!(
            overrides.runner_cli.plan_mode,
            Some(RunnerPlanMode::Enabled)
        );
    }

    #[test]
    fn resolve_run_agent_overrides_can_disable_update_task_via_cli() {
        let args = RunAgentArgs {
            runner: None,
            model: None,
            effort: None,
            runner_cli: RunnerCliArgs::default(),
            phases: None,
            quick: false,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: true,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            no_progress: false,
            notify_sound: false,
            lfs_check: false,
            runner_phase1: None,
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();
        assert_eq!(overrides.update_task_before_run, Some(false));
    }

    #[test]
    fn resolve_run_agent_overrides_quick_flag_sets_phases_to_one() {
        let args = RunAgentArgs {
            runner: None,
            model: None,
            effort: None,
            runner_cli: RunnerCliArgs::default(),
            phases: None,
            quick: true,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: None,
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();
        assert_eq!(overrides.phases, Some(1));
    }

    #[test]
    fn resolve_run_agent_overrides_phases_override_takes_precedence_when_quick_false() {
        let args = RunAgentArgs {
            runner: None,
            model: None,
            effort: None,
            runner_cli: RunnerCliArgs::default(),
            phases: Some(3),
            quick: false,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: None,
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();
        assert_eq!(overrides.phases, Some(3));
    }

    #[test]
    fn resolve_run_agent_overrides_phase_flags_parsed_correctly() {
        let args = RunAgentArgs {
            runner: Some("claude".to_string()),
            model: Some("sonnet".to_string()),
            effort: None,
            runner_cli: RunnerCliArgs::default(),
            phases: Some(3),
            quick: false,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: Some("codex".to_string()),
            model_phase1: Some("gpt-5.2-codex".to_string()),
            effort_phase1: Some("high".to_string()),
            runner_phase2: Some("claude".to_string()),
            model_phase2: Some("opus".to_string()),
            effort_phase2: None,
            runner_phase3: Some("codex".to_string()),
            model_phase3: Some("gpt-5.2-codex".to_string()),
            effort_phase3: Some("medium".to_string()),
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();

        // Global overrides should still be set
        assert_eq!(overrides.runner, Some(Runner::Claude));
        assert_eq!(overrides.model, Some(Model::Custom("sonnet".to_string())));

        // Phase overrides should be populated
        let phase_overrides = overrides
            .phase_overrides
            .expect("phase_overrides should be set");

        // Phase 1
        let phase1 = phase_overrides.phase1.expect("phase1 should be set");
        assert_eq!(phase1.runner, Some(Runner::Codex));
        assert_eq!(phase1.model, Some(Model::Gpt52Codex));
        assert_eq!(phase1.reasoning_effort, Some(ReasoningEffort::High));

        // Phase 2
        let phase2 = phase_overrides.phase2.expect("phase2 should be set");
        assert_eq!(phase2.runner, Some(Runner::Claude));
        assert_eq!(phase2.model, Some(Model::Custom("opus".to_string())));
        assert_eq!(phase2.reasoning_effort, None);

        // Phase 3
        let phase3 = phase_overrides.phase3.expect("phase3 should be set");
        assert_eq!(phase3.runner, Some(Runner::Codex));
        assert_eq!(phase3.model, Some(Model::Gpt52Codex));
        assert_eq!(phase3.reasoning_effort, Some(ReasoningEffort::Medium));
    }

    #[test]
    fn resolve_run_agent_overrides_phase_flags_partial() {
        // Test that partial phase overrides work (e.g., only --runner-phase1)
        let args = RunAgentArgs {
            runner: None,
            model: None,
            effort: None,
            runner_cli: RunnerCliArgs::default(),
            phases: None,
            quick: false,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: Some("codex".to_string()),
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();

        let phase_overrides = overrides
            .phase_overrides
            .expect("phase_overrides should be set");

        // Only phase1 should be set
        let phase1 = phase_overrides.phase1.expect("phase1 should be set");
        assert_eq!(phase1.runner, Some(Runner::Codex));
        assert_eq!(phase1.model, None);
        assert_eq!(phase1.reasoning_effort, None);

        // Phase 2 and 3 should be None
        assert!(phase_overrides.phase2.is_none());
        assert!(phase_overrides.phase3.is_none());
    }

    #[test]
    fn resolve_run_agent_overrides_empty_phase_flags_returns_none() {
        // Test that no phase flags results in phase_overrides: None
        let args = RunAgentArgs {
            runner: None,
            model: None,
            effort: None,
            runner_cli: RunnerCliArgs::default(),
            phases: None,
            quick: false,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: None,
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let overrides = resolve_run_agent_overrides(&args).unwrap();
        assert!(overrides.phase_overrides.is_none());
    }

    #[test]
    fn resolve_run_agent_overrides_invalid_runner_phase_includes_phase_in_error() {
        // Test that invalid runner for a phase produces error
        let args = RunAgentArgs {
            runner: None,
            model: None,
            effort: None,
            runner_cli: RunnerCliArgs::default(),
            phases: None,
            quick: false,
            repo_prompt: None,
            git_revert_mode: None,
            git_commit_push_on: false,
            git_commit_push_off: false,
            include_draft: false,
            update_task: false,
            no_update_task: false,
            notify: false,
            no_notify: false,
            notify_fail: false,
            no_notify_fail: false,
            notify_sound: false,
            lfs_check: false,
            no_progress: false,
            runner_phase1: Some("invalid_runner".to_string()),
            model_phase1: None,
            effort_phase1: None,
            runner_phase2: None,
            model_phase2: None,
            effort_phase2: None,
            runner_phase3: None,
            model_phase3: None,
            effort_phase3: None,
        };

        let result = resolve_run_agent_overrides(&args);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid runner"));
    }
}
