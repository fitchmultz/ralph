//! Plugin executor regression coverage.
//!
//! Purpose:
//! - Plugin executor regression coverage.
//!
//! Responsibilities:
//! - Verify executor metadata, managed-session rules, and final-response extraction.
//! - Lock down executor behavior for built-in and external plugin runners.
//!
//! Non-scope:
//! - Built-in command construction or per-line parser unit behavior.
//! - External plugin runtime execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - External plugins default to resume support and self-managed sessions.
//! - Final-response extraction should prefer the last assistant response in multiline output.

use super::*;

// =============================================================================
// PluginExecutor Tests
// =============================================================================

#[test]
fn plugin_executor_creates_with_all_built_ins() {
    let executor = PluginExecutor::new();

    for runner in [
        Runner::Codex,
        Runner::Opencode,
        Runner::Gemini,
        Runner::Claude,
        Runner::Kimi,
        Runner::Pi,
        Runner::Cursor,
    ] {
        let metadata: RunnerMetadata = executor.metadata(&runner);
        assert!(
            !metadata.id.is_empty(),
            "Runner {:?} should have metadata",
            runner
        );
    }
}

#[test]
fn plugin_executor_external_plugin_metadata() {
    let executor = PluginExecutor::new();
    let runner = Runner::Plugin("test.plugin".to_string());
    let metadata: RunnerMetadata = executor.metadata(&runner);

    assert_eq!(metadata.id, "test.plugin");
    assert!(
        metadata.supports_resume,
        "External plugins assume resume support"
    );
}

#[test]
fn plugin_executor_requires_managed_session_id() {
    let executor = PluginExecutor::new();

    assert!(executor.requires_managed_session_id(&Runner::Kimi));
    assert!(!executor.requires_managed_session_id(&Runner::Codex));
    assert!(!executor.requires_managed_session_id(&Runner::Claude));
    assert!(!executor.requires_managed_session_id(&Runner::Gemini));
    assert!(!executor.requires_managed_session_id(&Runner::Opencode));
    assert!(!executor.requires_managed_session_id(&Runner::Pi));
    assert!(!executor.requires_managed_session_id(&Runner::Cursor));
}

#[test]
fn plugin_executor_external_plugins_do_not_require_managed_session() {
    let executor = PluginExecutor::new();
    let runner = Runner::Plugin("external".to_string());

    assert!(
        !executor.requires_managed_session_id(&runner),
        "External plugins should manage their own session IDs"
    );
}

#[test]
fn plugin_executor_extract_final_response_codex() {
    let executor = PluginExecutor::new();
    let runner = Runner::Codex;

    let stdout =
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Final response"}}"#;
    let result = executor.extract_final_response(&runner, stdout);

    assert_eq!(result, Some("Final response".to_string()));
}

#[test]
fn plugin_executor_extract_final_response_kimi() {
    let executor = PluginExecutor::new();
    let runner = Runner::Kimi;

    let stdout = r#"{"role":"assistant","content":[{"type":"text","text":"Kimi response"}]}"#;
    let result = executor.extract_final_response(&runner, stdout);

    assert_eq!(result, Some("Kimi response".to_string()));
}

#[test]
fn plugin_executor_extract_final_response_multiline() {
    let executor = PluginExecutor::new();
    let runner = Runner::Claude;

    let stdout = r#"
{"type":"progress","message":"Processing..."}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"First part"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Second part"}]}}
"#;
    let result = executor.extract_final_response(&runner, stdout);

    // Should return the last assistant message
    assert_eq!(result, Some("Second part".to_string()));
}
