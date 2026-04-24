//! Regression tests for plugin-executor metadata and session-policy helpers.
//!
//! Purpose:
//! - Regression tests for plugin-executor metadata and session-policy helpers.
//!
//! Responsibilities:
//! - Confirm built-in runner registration remains complete.
//! - Verify managed-session behavior for built-in runners.
//! - Validate metadata shaping for external plugin runners.
//!
//! Not handled here:
//! - End-to-end runner subprocess execution.
//! - Response parsing details covered by execution test suites.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Built-in runner metadata must always be available from a fresh executor.
//! - External plugin runners default to resume support.

use super::*;

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
        let metadata = executor.metadata(&runner);
        assert!(!metadata.id.is_empty());
    }
}

#[test]
fn plugin_executor_kimi_requires_managed_session() {
    let executor = PluginExecutor::new();
    assert!(executor.requires_managed_session_id(&Runner::Kimi));
    assert!(!executor.requires_managed_session_id(&Runner::Codex));
}

#[test]
fn plugin_executor_external_plugin_metadata() {
    let executor = PluginExecutor::new();
    let runner = Runner::Plugin("test.plugin".to_string());
    let metadata = executor.metadata(&runner);

    assert_eq!(metadata.id, "test.plugin");
    assert!(metadata.supports_resume);
}
