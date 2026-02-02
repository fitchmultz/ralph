//! Unit tests for contracts defaults and config types.

use ralph::contracts::{ClaudePermissionMode, Config, Model, ProjectType, ReasoningEffort, Runner};
use std::path::PathBuf;

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.version, 1);
    assert_eq!(config.project_type, Some(ProjectType::Code));
    assert_eq!(config.queue.file, Some(PathBuf::from(".ralph/queue.json")));
    assert_eq!(
        config.queue.done_file,
        Some(PathBuf::from(".ralph/done.json"))
    );
    assert_eq!(config.queue.id_prefix, Some("RQ".to_string()));
    assert_eq!(config.queue.id_width, Some(4));
    assert_eq!(config.agent.runner, Some(Runner::Claude));
    assert_eq!(
        config.agent.model,
        Some(Model::Custom("sonnet".to_string()))
    );
    assert_eq!(config.agent.reasoning_effort, Some(ReasoningEffort::Medium));
    assert_eq!(config.agent.codex_bin, Some("codex".to_string()));
    assert_eq!(config.agent.opencode_bin, Some("opencode".to_string()));
    assert_eq!(config.agent.gemini_bin, Some("gemini".to_string()));
    assert_eq!(config.agent.claude_bin, Some("claude".to_string()));
    assert_eq!(
        config.agent.claude_permission_mode,
        Some(ClaudePermissionMode::BypassPermissions)
    );
    assert_eq!(config.agent.repoprompt_plan_required, Some(false));
    assert_eq!(config.agent.repoprompt_tool_injection, Some(false));
    assert_eq!(config.agent.phases, Some(3));
}
