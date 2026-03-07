//! Tests for configuration contracts.
//!
//! Responsibilities:
//! - Test config merge behavior and serialization.
//!
//! Not handled here:
//! - Integration tests (see `crates/ralph/tests/`).

use crate::contracts::{
    AgentConfig, Config, GitRevertMode, Model, NotificationConfig, PhaseOverrideConfig,
    PhaseOverrides, ReasoningEffort, Runner, RunnerRetryConfig, WebhookConfig,
};

#[test]
fn parallel_config_rejects_legacy_worktree_root_key() {
    let raw = r#"{
            "version": 1,
            "parallel": { "worktree_root": ".ralph/worktrees/custom" }
        }"#;
    let err = serde_json::from_str::<Config>(raw).unwrap_err();
    assert!(err.to_string().contains("worktree_root"));
}

#[test]
fn git_revert_mode_parses_snake_case() {
    let mode: GitRevertMode = serde_json::from_str("\"ask\"").expect("ask");
    assert_eq!(mode, GitRevertMode::Ask);
    let mode: GitRevertMode = serde_json::from_str("\"enabled\"").expect("enabled");
    assert_eq!(mode, GitRevertMode::Enabled);
    let mode: GitRevertMode = serde_json::from_str("\"disabled\"").expect("disabled");
    assert_eq!(mode, GitRevertMode::Disabled);
}

#[test]
fn git_revert_mode_from_str_rejects_invalid() {
    let err = "wat".parse::<GitRevertMode>().expect_err("invalid");
    assert!(err.contains("git_revert_mode"));
}

#[test]
fn test_phase_override_config_merge_from() {
    let mut base = PhaseOverrideConfig {
        runner: Some(Runner::Codex),
        model: None,
        reasoning_effort: Some(ReasoningEffort::Medium),
    };

    let override_config = PhaseOverrideConfig {
        runner: Some(Runner::Claude),
        model: Some(Model::Custom("claude-opus-4".to_string())),
        reasoning_effort: None,
    };

    base.merge_from(override_config);

    assert_eq!(base.runner, Some(Runner::Claude)); // overridden
    assert_eq!(base.model, Some(Model::Custom("claude-opus-4".to_string()))); // set
    assert_eq!(base.reasoning_effort, Some(ReasoningEffort::Medium)); // preserved
}

#[test]
fn test_phase_overrides_merge_from() {
    let mut base = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: Some(Runner::Codex),
            model: Some(Model::Custom("o3-mini".to_string())),
            reasoning_effort: None,
        }),
        phase2: None,
        phase3: None,
    };

    let override_config = PhaseOverrides {
        phase1: Some(PhaseOverrideConfig {
            runner: None,
            model: Some(Model::Custom("claude-sonnet".to_string())),
            reasoning_effort: Some(ReasoningEffort::High),
        }),
        phase2: Some(PhaseOverrideConfig {
            runner: Some(Runner::Gemini),
            model: None,
            reasoning_effort: None,
        }),
        phase3: None,
    };

    base.merge_from(override_config);

    // phase1 merged
    assert_eq!(base.phase1.as_ref().unwrap().runner, Some(Runner::Codex)); // preserved
    assert_eq!(
        base.phase1.as_ref().unwrap().model,
        Some(Model::Custom("claude-sonnet".to_string()))
    ); // overridden
    assert_eq!(
        base.phase1.as_ref().unwrap().reasoning_effort,
        Some(ReasoningEffort::High)
    ); // set

    // phase2 set from override
    assert_eq!(base.phase2.as_ref().unwrap().runner, Some(Runner::Gemini));

    // phase3 still None
    assert!(base.phase3.is_none());
}

#[test]
fn test_agent_config_phase_overrides_merge() {
    let mut base = AgentConfig {
        runner: Some(Runner::Codex),
        model: Some(Model::Custom("o3-mini".to_string())),
        reasoning_effort: Some(ReasoningEffort::Medium),
        phases: Some(3),
        iterations: None,
        followup_reasoning_effort: None,
        codex_bin: None,
        opencode_bin: None,
        gemini_bin: None,
        claude_bin: None,
        cursor_bin: None,
        kimi_bin: None,
        pi_bin: None,
        claude_permission_mode: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase1: None,
            phase2: None,
            phase3: None,
        }),
        instruction_files: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        ci_gate: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        notification: NotificationConfig::default(),
        webhook: WebhookConfig::default(),
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: None,
        scan_prompt_version: None,
    };

    let override_config = AgentConfig {
        runner: Some(Runner::Claude),
        model: Some(Model::Custom("claude-sonnet".to_string())),
        reasoning_effort: Some(ReasoningEffort::High),
        phases: None,
        iterations: None,
        followup_reasoning_effort: None,
        codex_bin: None,
        opencode_bin: None,
        gemini_bin: None,
        claude_bin: None,
        cursor_bin: None,
        kimi_bin: None,
        pi_bin: None,
        claude_permission_mode: None,
        runner_cli: None,
        phase_overrides: Some(PhaseOverrides {
            phase1: Some(PhaseOverrideConfig {
                runner: None,
                model: Some(Model::Custom("claude-opus-4".to_string())),
                reasoning_effort: Some(ReasoningEffort::XHigh),
            }),
            phase2: None,
            phase3: None,
        }),
        instruction_files: None,
        repoprompt_plan_required: None,
        repoprompt_tool_injection: None,
        ci_gate: None,
        git_revert_mode: None,
        git_commit_push_enabled: None,
        notification: NotificationConfig::default(),
        webhook: WebhookConfig::default(),
        runner_retry: RunnerRetryConfig::default(),
        session_timeout_hours: None,
        scan_prompt_version: None,
    };

    base.merge_from(override_config);

    // Verify global settings merged
    assert_eq!(base.runner, Some(Runner::Claude));
    assert_eq!(base.model, Some(Model::Custom("claude-sonnet".to_string())));
    assert_eq!(base.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(base.phases, Some(3)); // preserved

    // Verify phase_overrides merged
    let phase1 = base
        .phase_overrides
        .as_ref()
        .unwrap()
        .phase1
        .as_ref()
        .unwrap();
    assert_eq!(phase1.runner, None); // preserved (None in override)
    assert_eq!(
        phase1.model,
        Some(Model::Custom("claude-opus-4".to_string()))
    );
    assert_eq!(phase1.reasoning_effort, Some(ReasoningEffort::XHigh));
}
