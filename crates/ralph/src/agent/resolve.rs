//! Agent override resolution from CLI arguments.
//!
//! Responsibilities:
//! - Resolve agent overrides from CLI arguments for run commands (with phases).
//! - Resolve agent overrides from CLI arguments for scan/task commands (simpler version).
//! - Define AgentOverrides struct holding all resolved override values.
//!
//! Not handled here:
//! - CLI argument struct definitions (see `super::args`).
//! - Parsing functions (see `super::parse`).
//! - RepoPrompt flag resolution (see `super::repoprompt`).
//!
//! Invariants/assumptions:
//! - Override resolution validates runner/model compatibility.
//! - Phase-specific overrides are only populated when at least one phase flag is set.
//! - The --quick flag overrides --phases to set phases=1.

use crate::config;
use crate::contracts::{
    GitRevertMode, Model, PhaseOverrideConfig, PhaseOverrides, ReasoningEffort, Runner,
    RunnerCliOptionsPatch,
};
use crate::runner;
use anyhow::Result;

use super::args::{AgentArgs, RunAgentArgs};
use super::parse::{parse_git_revert_mode, parse_runner, parse_runner_cli_patch};
use super::repoprompt::{
    RepopromptFlags, repoprompt_flags_from_mode, resolve_repoprompt_flags_from_agent_config,
};

/// Helper macro to resolve a boolean CLI flag with enable/disable variants.
///
/// Takes the enable flag expression and disable flag expression, returns
/// `Some(true)` if enabled, `Some(false)` if disabled, or `None` if neither.
macro_rules! resolve_bool_flag {
    ($enable:expr, $disable:expr) => {
        if $enable {
            Some(true)
        } else if $disable {
            Some(false)
        } else {
            None
        }
    };
}

/// Helper macro to resolve a simple optional boolean flag.
///
/// Returns `Some(true)` if the flag is set, `None` otherwise.
macro_rules! resolve_simple_flag {
    ($flag:expr) => {
        if $flag { Some(true) } else { None }
    };
}

/// Agent overrides from CLI arguments.
///
/// These overrides take precedence over task.agent and config defaults.
#[derive(Debug, Clone, Default)]
pub struct AgentOverrides {
    /// Named configuration profile to apply.
    pub profile: Option<String>,
    pub runner: Option<Runner>,
    pub model: Option<Model>,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub runner_cli: RunnerCliOptionsPatch,
    /// Execution shape override:
    /// - 1 => single-pass execution
    /// - 2 => two-pass execution (plan then implement)
    /// - 3 => three-pass execution (plan, implement+CI, review+complete)
    pub phases: Option<u8>,
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

/// Resolve agent overrides from CLI arguments for run commands.
///
/// This parses the CLI arguments and validates runner/model compatibility.
pub fn resolve_run_agent_overrides(args: &RunAgentArgs) -> Result<AgentOverrides> {
    use crate::runner;

    let profile = args.profile.clone();

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

    if let (Some(runner_kind), Some(model)) = (runner.as_ref(), model.as_ref()) {
        runner::validate_model_for_runner(runner_kind, model)?;
    }

    let repoprompt_override = args.repo_prompt.map(repoprompt_flags_from_mode);

    let git_revert_mode = match args.git_revert_mode.as_deref() {
        Some(value) => Some(parse_git_revert_mode(value)?),
        None => None,
    };

    let git_commit_push_enabled =
        resolve_bool_flag!(args.git_commit_push_on, args.git_commit_push_off);
    let include_draft = resolve_simple_flag!(args.include_draft);

    // Handle --quick flag: when set, override phases to 1 (single-pass execution)
    let phases = if args.quick { Some(1) } else { args.phases };

    // Handle notification flags
    let notify_on_complete = resolve_bool_flag!(args.notify, args.no_notify);
    let notify_on_fail = resolve_bool_flag!(args.notify_fail, args.no_notify_fail);
    let notify_sound = resolve_simple_flag!(args.notify_sound);
    let lfs_check = resolve_simple_flag!(args.lfs_check);
    let no_progress = resolve_simple_flag!(args.no_progress);

    // Parse phase-specific overrides using helper to avoid duplication
    let phase_overrides = resolve_phase_overrides(args)?;

    Ok(AgentOverrides {
        profile,
        runner,
        model,
        reasoning_effort,
        runner_cli,
        phases,
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

    if let (Some(runner_kind), Some(model)) = (runner.as_ref(), model.as_ref()) {
        runner::validate_model_for_runner(runner_kind, model)?;
    }

    let repoprompt_override = args.repo_prompt.map(repoprompt_flags_from_mode);
    let runner_cli = parse_runner_cli_patch(&args.runner_cli)?;

    Ok(AgentOverrides {
        profile: None,
        runner,
        model,
        reasoning_effort,
        runner_cli,
        phases: None,
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

/// Helper to resolve phase overrides for a single phase.
///
/// Takes optional runner, model, and effort strings and returns a PhaseOverrideConfig
/// if any are provided. This eliminates DRY violations in the main resolution function.
fn resolve_single_phase_override(
    runner: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Option<PhaseOverrideConfig>> {
    if runner.is_none() && model.is_none() && effort.is_none() {
        return Ok(None);
    }

    Ok(Some(PhaseOverrideConfig {
        runner: runner.map(parse_runner).transpose()?,
        model: model.map(runner::parse_model).transpose()?,
        reasoning_effort: effort.map(runner::parse_reasoning_effort).transpose()?,
    }))
}

/// Resolve phase-specific overrides from CLI arguments.
///
/// This centralizes the phase override resolution to eliminate the DRY violation
/// of having nearly identical code blocks for each phase.
fn resolve_phase_overrides(args: &RunAgentArgs) -> Result<Option<PhaseOverrides>> {
    let phase1 = resolve_single_phase_override(
        args.runner_phase1.as_deref(),
        args.model_phase1.as_deref(),
        args.effort_phase1.as_deref(),
    )?;
    let phase2 = resolve_single_phase_override(
        args.runner_phase2.as_deref(),
        args.model_phase2.as_deref(),
        args.effort_phase2.as_deref(),
    )?;
    let phase3 = resolve_single_phase_override(
        args.runner_phase3.as_deref(),
        args.model_phase3.as_deref(),
        args.effort_phase3.as_deref(),
    )?;

    if phase1.is_none() && phase2.is_none() && phase3.is_none() {
        Ok(None)
    } else {
        Ok(Some(PhaseOverrides {
            phase1,
            phase2,
            phase3,
        }))
    }
}

/// Resolve RepoPrompt flags from overrides, falling back to config.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::args::RunnerCliArgs;
    use crate::contracts::{
        GitRevertMode, Model, ReasoningEffort, Runner, RunnerApprovalMode, RunnerPlanMode,
        RunnerSandboxMode,
    };

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
        use crate::agent::repoprompt::RepoPromptMode;
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
        use crate::agent::repoprompt::RepoPromptMode;
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
            profile: None,
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
        assert_eq!(overrides.git_revert_mode, Some(GitRevertMode::Enabled));
        assert_eq!(overrides.git_commit_push_enabled, Some(false));
        assert_eq!(overrides.include_draft, Some(true));
    }

    #[test]
    fn resolve_run_agent_overrides_parses_runner_cli_args() {
        let args = RunAgentArgs {
            profile: None,
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
    fn resolve_run_agent_overrides_quick_flag_sets_phases_to_one() {
        let args = RunAgentArgs {
            profile: None,
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
            profile: None,
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
            profile: None,
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
            profile: None,
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
            profile: None,
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
            profile: None,
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
