//! Unit tests for contracts defaults and config types.
//!
//! Purpose:
//! - Unit tests for contracts defaults and config types.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use ralph::contracts::{
    ClaudePermissionMode, Config, GitPublishMode, Model, ProjectType, ReasoningEffort, Runner,
    RunnerApprovalMode,
};
use std::path::PathBuf;

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.version, 2);
    assert_eq!(config.project_type, Some(ProjectType::Code));
    assert_eq!(config.queue.file, Some(PathBuf::from(".ralph/queue.jsonc")));
    assert_eq!(
        config.queue.done_file,
        Some(PathBuf::from(".ralph/done.jsonc"))
    );
    assert_eq!(config.queue.id_prefix, Some("RQ".to_string()));
    assert_eq!(config.queue.id_width, Some(4));
    assert_eq!(config.agent.runner, Some(Runner::Codex));
    assert_eq!(config.agent.model, Some(Model::Gpt54));
    assert_eq!(config.agent.reasoning_effort, Some(ReasoningEffort::Medium));
    assert_eq!(config.agent.codex_bin, Some("codex".to_string()));
    assert_eq!(config.agent.opencode_bin, Some("opencode".to_string()));
    assert_eq!(config.agent.gemini_bin, Some("gemini".to_string()));
    assert_eq!(config.agent.claude_bin, Some("claude".to_string()));
    assert_eq!(
        config.agent.claude_permission_mode,
        Some(ClaudePermissionMode::AcceptEdits)
    );
    assert_eq!(config.agent.git_publish_mode, Some(GitPublishMode::Off));
    assert_eq!(
        config
            .agent
            .runner_cli
            .as_ref()
            .and_then(|cli| cli.defaults.approval_mode),
        Some(RunnerApprovalMode::Default)
    );
    assert_eq!(config.agent.repoprompt_plan_required, Some(false));
    assert_eq!(config.agent.repoprompt_tool_injection, Some(false));
    assert_eq!(config.agent.phases, Some(3));
    assert!(config.profiles.is_none());
}
