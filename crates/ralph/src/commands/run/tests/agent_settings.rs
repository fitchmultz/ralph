//! Agent settings resolution tests for run command.

use super::{LoggerState, base_task, resolved_with_agent_defaults, take_logs};
use crate::agent::AgentOverrides;
use crate::contracts::{
    Model, ModelEffort, ReasoningEffort, Runner, RunnerCliOptionsPatch, TaskAgent,
};
use crate::runner;

#[test]
fn resolve_run_agent_settings_task_agent_overrides_config() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        model_effort: ModelEffort::High,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    });

    let overrides = AgentOverrides::default();
    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Opencode);
    assert_eq!(settings.model, Model::Gpt52);
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_run_agent_settings_cli_overrides_task_agent_and_config() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Opencode),
        Some(Model::Gpt52),
        Some(ReasoningEffort::Low),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        model_effort: ModelEffort::Low,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    });

    let overrides = AgentOverrides {
        profile: None,
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: RunnerCliOptionsPatch::default(),
        phases: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
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

    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Codex);
    assert_eq!(settings.model, Model::Gpt52Codex);
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::High));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_defaults_to_glm47_for_opencode_runner() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
    );

    let task = base_task();

    let overrides = AgentOverrides {
        profile: None,
        runner: Some(Runner::Opencode),
        model: None,
        reasoning_effort: None,
        runner_cli: RunnerCliOptionsPatch::default(),
        phases: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
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

    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Opencode);
    assert_eq!(settings.model, Model::Glm47);
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_run_agent_settings_defaults_to_gemini_flash_for_gemini_runner() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Medium),
    );

    let task = base_task();

    let overrides = AgentOverrides {
        profile: None,
        runner: Some(Runner::Gemini),
        model: None,
        reasoning_effort: None,
        runner_cli: RunnerCliOptionsPatch::default(),
        phases: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
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

    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Gemini);
    assert_eq!(settings.model.as_str(), "gemini-3-flash-preview");
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_run_agent_settings_effort_defaults_to_medium_for_codex_when_unspecified()
-> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(Some(Runner::Codex), Some(Model::Gpt52Codex), None);

    let task = base_task();
    let overrides = AgentOverrides::default();

    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Codex);
    assert_eq!(settings.model, Model::Gpt52Codex);
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::Medium));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_model_effort_default_uses_config() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::High),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    });

    let overrides = AgentOverrides::default();
    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::High));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_model_effort_overrides_config_for_codex() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Codex),
        Some(Model::Gpt52Codex),
        Some(ReasoningEffort::Low),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        model_effort: ModelEffort::XHigh,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    });

    let overrides = AgentOverrides::default();
    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.reasoning_effort, Some(ReasoningEffort::XHigh));
    Ok(())
}

#[test]
fn resolve_run_agent_settings_effort_is_ignored_for_opencode() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(
        Some(Runner::Opencode),
        Some(Model::Gpt52),
        Some(ReasoningEffort::Low),
    );

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        model_effort: ModelEffort::High,
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        runner_cli: None,
        phase_overrides: None,
    });
    let overrides = AgentOverrides {
        profile: None,
        runner: Some(Runner::Opencode),
        model: Some(Model::Gpt52),
        reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: RunnerCliOptionsPatch::default(),
        phases: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
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

    let settings = crate::commands::run::resolve_run_agent_settings(&resolved, &task, &overrides)?;
    assert_eq!(settings.runner, Runner::Opencode);
    assert_eq!(settings.model, Model::Gpt52);
    assert_eq!(settings.reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_iteration_settings_defaults_to_one() -> anyhow::Result<()> {
    let resolved = resolved_with_agent_defaults(None, None, None);
    let task = base_task();

    let settings = crate::commands::run::resolve_iteration_settings(&task, &resolved.config.agent)?;
    assert_eq!(settings.count, 1);
    assert_eq!(settings.followup_reasoning_effort, None);
    Ok(())
}

#[test]
fn resolve_iteration_settings_prefers_task_over_config() -> anyhow::Result<()> {
    let mut resolved = resolved_with_agent_defaults(None, None, None);
    resolved.config.agent.iterations = Some(3);
    resolved.config.agent.followup_reasoning_effort = Some(ReasoningEffort::Low);

    let mut task = base_task();
    task.agent = Some(TaskAgent {
        runner: None,
        model: None,
        model_effort: ModelEffort::Default,
        phases: None,
        iterations: Some(2),
        followup_reasoning_effort: Some(ReasoningEffort::High),
        runner_cli: None,
        phase_overrides: None,
    });

    let settings = crate::commands::run::resolve_iteration_settings(&task, &resolved.config.agent)?;
    assert_eq!(settings.count, 2);
    assert_eq!(
        settings.followup_reasoning_effort,
        Some(ReasoningEffort::High)
    );
    Ok(())
}

#[test]
fn apply_followup_reasoning_effort_overrides_codex_only() {
    let base = runner::AgentSettings {
        runner: Runner::Codex,
        model: Model::Gpt52Codex,
        reasoning_effort: Some(ReasoningEffort::Medium),
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let updated = crate::commands::run::apply_followup_reasoning_effort(
        &base,
        Some(ReasoningEffort::High),
        true,
    );
    assert_eq!(updated.reasoning_effort, Some(ReasoningEffort::High));

    let base_non_codex = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Glm47,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };
    let updated_non_codex = crate::commands::run::apply_followup_reasoning_effort(
        &base_non_codex,
        Some(ReasoningEffort::High),
        true,
    );
    assert_eq!(updated_non_codex.reasoning_effort, None);
}

#[test]
fn apply_followup_reasoning_effort_warns_for_non_codex() {
    let base_non_codex = runner::AgentSettings {
        runner: Runner::Opencode,
        model: Model::Glm47,
        reasoning_effort: None,
        runner_cli: runner::ResolvedRunnerCliOptions::default(),
    };

    let (state, _) = take_logs();
    let _ = crate::commands::run::apply_followup_reasoning_effort(
        &base_non_codex,
        Some(ReasoningEffort::High),
        true,
    );
    let (_, logs) = take_logs();

    if state == LoggerState::TestLogger {
        assert!(
            logs.iter()
                .any(|entry| entry.contains("Follow-up reasoning_effort configured")),
            "expected warning log, got {logs:?}"
        );
    }
}

#[test]
fn task_context_block_includes_id_and_title() -> anyhow::Result<()> {
    let mut t = base_task();
    t.id = "RQ-0001".to_string();
    t.title = "Hello world".to_string();
    let rendered = crate::commands::run::task_context_for_prompt(&t)?;
    assert!(rendered.contains("RQ-0001"));
    assert!(rendered.contains("Hello world"));
    assert!(rendered.contains("Raw task JSON"));
    Ok(())
}
