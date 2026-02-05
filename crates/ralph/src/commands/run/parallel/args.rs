//! CLI argument builders for parallel worker subprocesses.
//!
//! Responsibilities:
//! - Convert override configurations into CLI argument vectors.
//! - Map enum values to their CLI string representations.
//! - Handle phase-specific overrides for multi-phase execution.
//!
//! Not handled here:
//! - Worker process management (see `super::worker`).
//! - State synchronization (see `super::sync`).
//!
//! Invariants/assumptions:
//! - Argument names match the CLI definition in `crate::cli`.

use crate::agent::AgentOverrides;
use crate::contracts::RunnerCliOptionsPatch;

/// Build CLI arguments from agent overrides.
pub(crate) fn build_override_args(overrides: &AgentOverrides) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(runner) = overrides.runner.clone() {
        args.push("--runner".to_string());
        args.push(runner.as_str().to_string());
    }
    if let Some(model) = overrides.model.clone() {
        args.push("--model".to_string());
        args.push(model.as_str().to_string());
    }
    if let Some(effort) = overrides.reasoning_effort {
        args.push("--effort".to_string());
        args.push(reasoning_effort_arg(effort).to_string());
    }
    if let Some(phases) = overrides.phases {
        args.push("--phases".to_string());
        args.push(phases.to_string());
    }
    if let Some(repo_prompt) = repo_prompt_arg(overrides) {
        args.push("--repo-prompt".to_string());
        args.push(repo_prompt.to_string());
    }
    if let Some(mode) = overrides.git_revert_mode {
        args.push("--git-revert-mode".to_string());
        args.push(git_revert_mode_arg(mode).to_string());
    }

    if let Some(value) = overrides.git_commit_push_enabled {
        args.push(if value {
            "--git-commit-push-on".to_string()
        } else {
            "--git-commit-push-off".to_string()
        });
    }

    if overrides.include_draft.unwrap_or(false) {
        args.push("--include-draft".to_string());
    }

    if let Some(update) = overrides.update_task_before_run {
        if update {
            args.push("--update-task".to_string());
        } else {
            args.push("--no-update-task".to_string());
        }
    }

    if let Some(value) = overrides.notify_on_complete {
        args.push(if value {
            "--notify".to_string()
        } else {
            "--no-notify".to_string()
        });
    }

    if let Some(value) = overrides.notify_on_fail {
        args.push(if value {
            "--notify-fail".to_string()
        } else {
            "--no-notify-fail".to_string()
        });
    }

    if overrides.notify_sound.unwrap_or(false) {
        args.push("--notify-sound".to_string());
    }

    if overrides.lfs_check.unwrap_or(false) {
        args.push("--lfs-check".to_string());
    }

    if let Some(cli) = build_runner_cli_args(&overrides.runner_cli) {
        args.extend(cli);
    }

    if let Some(phase_args) = build_phase_override_args(overrides) {
        args.extend(phase_args);
    }

    args
}

/// Build CLI arguments from runner CLI options patch.
pub(crate) fn build_runner_cli_args(cli: &RunnerCliOptionsPatch) -> Option<Vec<String>> {
    let mut args = Vec::new();
    if let Some(value) = cli.output_format {
        args.push("--output-format".to_string());
        args.push(output_format_arg(value).to_string());
    }
    if let Some(value) = cli.verbosity {
        args.push("--verbosity".to_string());
        args.push(verbosity_arg(value).to_string());
    }
    if let Some(value) = cli.approval_mode {
        args.push("--approval-mode".to_string());
        args.push(approval_mode_arg(value).to_string());
    }
    if let Some(value) = cli.sandbox {
        args.push("--sandbox".to_string());
        args.push(sandbox_mode_arg(value).to_string());
    }
    if let Some(value) = cli.plan_mode {
        args.push("--plan-mode".to_string());
        args.push(plan_mode_arg(value).to_string());
    }
    if let Some(value) = cli.unsupported_option_policy {
        args.push("--unsupported-option-policy".to_string());
        args.push(unsupported_option_policy_arg(value).to_string());
    }

    if args.is_empty() { None } else { Some(args) }
}

/// Build phase-specific CLI arguments from overrides.
pub(crate) fn build_phase_override_args(overrides: &AgentOverrides) -> Option<Vec<String>> {
    let overrides = overrides.phase_overrides.as_ref()?;
    let mut args = Vec::new();

    if let Some(phase1) = overrides.phase1.as_ref() {
        if let Some(runner) = phase1.runner.clone() {
            args.push("--runner-phase1".to_string());
            args.push(runner.as_str().to_string());
        }
        if let Some(model) = phase1.model.clone() {
            args.push("--model-phase1".to_string());
            args.push(model.as_str().to_string());
        }
        if let Some(effort) = phase1.reasoning_effort {
            args.push("--effort-phase1".to_string());
            args.push(reasoning_effort_arg(effort).to_string());
        }
    }

    if let Some(phase2) = overrides.phase2.as_ref() {
        if let Some(runner) = phase2.runner.clone() {
            args.push("--runner-phase2".to_string());
            args.push(runner.as_str().to_string());
        }
        if let Some(model) = phase2.model.clone() {
            args.push("--model-phase2".to_string());
            args.push(model.as_str().to_string());
        }
        if let Some(effort) = phase2.reasoning_effort {
            args.push("--effort-phase2".to_string());
            args.push(reasoning_effort_arg(effort).to_string());
        }
    }

    if let Some(phase3) = overrides.phase3.as_ref() {
        if let Some(runner) = phase3.runner.clone() {
            args.push("--runner-phase3".to_string());
            args.push(runner.as_str().to_string());
        }
        if let Some(model) = phase3.model.clone() {
            args.push("--model-phase3".to_string());
            args.push(model.as_str().to_string());
        }
        if let Some(effort) = phase3.reasoning_effort {
            args.push("--effort-phase3".to_string());
            args.push(reasoning_effort_arg(effort).to_string());
        }
    }

    if args.is_empty() { None } else { Some(args) }
}

fn repo_prompt_arg(overrides: &AgentOverrides) -> Option<&'static str> {
    match (
        overrides.repoprompt_plan_required,
        overrides.repoprompt_tool_injection,
    ) {
        (Some(true), Some(true)) => Some("plan"),
        (Some(false), Some(true)) => Some("tools"),
        (Some(false), Some(false)) => Some("off"),
        _ => None,
    }
}

fn reasoning_effort_arg(effort: crate::contracts::ReasoningEffort) -> &'static str {
    match effort {
        crate::contracts::ReasoningEffort::Low => "low",
        crate::contracts::ReasoningEffort::Medium => "medium",
        crate::contracts::ReasoningEffort::High => "high",
        crate::contracts::ReasoningEffort::XHigh => "xhigh",
    }
}

fn git_revert_mode_arg(mode: crate::contracts::GitRevertMode) -> &'static str {
    match mode {
        crate::contracts::GitRevertMode::Ask => "ask",
        crate::contracts::GitRevertMode::Enabled => "enabled",
        crate::contracts::GitRevertMode::Disabled => "disabled",
    }
}

fn output_format_arg(mode: crate::contracts::RunnerOutputFormat) -> &'static str {
    match mode {
        crate::contracts::RunnerOutputFormat::StreamJson => "stream-json",
        crate::contracts::RunnerOutputFormat::Json => "json",
        crate::contracts::RunnerOutputFormat::Text => "text",
    }
}

fn verbosity_arg(mode: crate::contracts::RunnerVerbosity) -> &'static str {
    match mode {
        crate::contracts::RunnerVerbosity::Quiet => "quiet",
        crate::contracts::RunnerVerbosity::Normal => "normal",
        crate::contracts::RunnerVerbosity::Verbose => "verbose",
    }
}

fn approval_mode_arg(mode: crate::contracts::RunnerApprovalMode) -> &'static str {
    match mode {
        crate::contracts::RunnerApprovalMode::Default => "default",
        crate::contracts::RunnerApprovalMode::AutoEdits => "auto-edits",
        crate::contracts::RunnerApprovalMode::Yolo => "yolo",
        crate::contracts::RunnerApprovalMode::Safe => "safe",
    }
}

fn sandbox_mode_arg(mode: crate::contracts::RunnerSandboxMode) -> &'static str {
    match mode {
        crate::contracts::RunnerSandboxMode::Default => "default",
        crate::contracts::RunnerSandboxMode::Enabled => "enabled",
        crate::contracts::RunnerSandboxMode::Disabled => "disabled",
    }
}

fn plan_mode_arg(mode: crate::contracts::RunnerPlanMode) -> &'static str {
    match mode {
        crate::contracts::RunnerPlanMode::Default => "default",
        crate::contracts::RunnerPlanMode::Enabled => "enabled",
        crate::contracts::RunnerPlanMode::Disabled => "disabled",
    }
}

fn unsupported_option_policy_arg(mode: crate::contracts::UnsupportedOptionPolicy) -> &'static str {
    match mode {
        crate::contracts::UnsupportedOptionPolicy::Ignore => "ignore",
        crate::contracts::UnsupportedOptionPolicy::Warn => "warn",
        crate::contracts::UnsupportedOptionPolicy::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{
        PhaseOverrideConfig, PhaseOverrides, ReasoningEffort, Runner, RunnerApprovalMode,
        RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
        UnsupportedOptionPolicy,
    };

    #[test]
    fn build_override_args_emits_expected_flags() {
        let overrides = AgentOverrides {
            runner: Some(Runner::Codex),
            model: Some(crate::contracts::Model::Gpt52),
            reasoning_effort: Some(ReasoningEffort::High),
            phases: Some(2),
            repoprompt_plan_required: Some(true),
            repoprompt_tool_injection: Some(true),
            git_revert_mode: Some(crate::contracts::GitRevertMode::Disabled),
            git_commit_push_enabled: Some(true),
            include_draft: Some(true),
            update_task_before_run: Some(false),
            notify_on_complete: Some(true),
            notify_on_fail: Some(false),
            notify_sound: Some(true),
            lfs_check: Some(true),
            ..Default::default()
        };

        let args = build_override_args(&overrides);
        let expected = vec![
            "--runner",
            "codex",
            "--model",
            "gpt-5.2",
            "--effort",
            "high",
            "--phases",
            "2",
            "--repo-prompt",
            "plan",
            "--git-revert-mode",
            "disabled",
            "--git-commit-push-on",
            "--include-draft",
            "--no-update-task",
            "--notify",
            "--no-notify-fail",
            "--notify-sound",
            "--lfs-check",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        assert_eq!(args, expected);
    }

    #[test]
    fn build_override_args_emits_git_commit_push_off() {
        let overrides = AgentOverrides {
            git_commit_push_enabled: Some(false),
            ..Default::default()
        };

        let args = build_override_args(&overrides);
        assert!(args.contains(&"--git-commit-push-off".to_string()));
        assert!(!args.contains(&"--git-commit-push-on".to_string()));
    }

    #[test]
    fn build_override_args_no_git_commit_push_flag_when_none() {
        let overrides = AgentOverrides {
            git_commit_push_enabled: None,
            ..Default::default()
        };

        let args = build_override_args(&overrides);
        assert!(!args.contains(&"--git-commit-push-on".to_string()));
        assert!(!args.contains(&"--git-commit-push-off".to_string()));
    }

    #[test]
    fn build_runner_cli_args_serializes_patch() {
        let patch = RunnerCliOptionsPatch {
            output_format: Some(RunnerOutputFormat::Json),
            verbosity: Some(RunnerVerbosity::Verbose),
            approval_mode: Some(RunnerApprovalMode::AutoEdits),
            sandbox: Some(RunnerSandboxMode::Disabled),
            plan_mode: Some(RunnerPlanMode::Enabled),
            unsupported_option_policy: Some(UnsupportedOptionPolicy::Error),
        };
        let args = build_runner_cli_args(&patch).expect("args");
        let expected = vec![
            "--output-format",
            "json",
            "--verbosity",
            "verbose",
            "--approval-mode",
            "auto-edits",
            "--sandbox",
            "disabled",
            "--plan-mode",
            "enabled",
            "--unsupported-option-policy",
            "error",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        assert_eq!(args, expected);
    }

    #[test]
    fn build_phase_override_args_serializes_phase_flags() {
        let overrides = PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: Some(Runner::Codex),
                model: Some(crate::contracts::Model::Gpt52Codex),
                reasoning_effort: Some(ReasoningEffort::Low),
            }),
            phase2: Some(PhaseOverrideConfig {
                runner: Some(Runner::Claude),
                model: Some(crate::contracts::Model::Gpt52),
                reasoning_effort: Some(ReasoningEffort::Medium),
            }),
            phase3: Some(PhaseOverrideConfig {
                runner: Some(Runner::Kimi),
                model: Some(crate::contracts::Model::Glm47),
                reasoning_effort: Some(ReasoningEffort::High),
            }),
        };
        let agent_overrides = AgentOverrides {
            phase_overrides: Some(overrides),
            ..Default::default()
        };

        let args = build_phase_override_args(&agent_overrides).expect("args");
        let expected = vec![
            "--runner-phase1",
            "codex",
            "--model-phase1",
            "gpt-5.2-codex",
            "--effort-phase1",
            "low",
            "--runner-phase2",
            "claude",
            "--model-phase2",
            "gpt-5.2",
            "--effort-phase2",
            "medium",
            "--runner-phase3",
            "kimi",
            "--model-phase3",
            "zai-coding-plan/glm-4.7",
            "--effort-phase3",
            "high",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        assert_eq!(args, expected);
    }
}
