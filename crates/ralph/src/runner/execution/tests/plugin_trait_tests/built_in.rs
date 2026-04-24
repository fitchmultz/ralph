//! Built-in plugin metadata regression coverage.
//!
//! Purpose:
//! - Built-in plugin metadata regression coverage.
//!
//! Responsibilities:
//! - Verify built-in plugin identity, metadata, resume support, and session rules.
//! - Lock down invariants shared across the built-in runner catalog.
//!
//! Non-scope:
//! - Command-building argument coverage.
//! - Response parsing or executor dispatch behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Built-in plugin ordering stays stable across the seven supported runners.
//! - Kimi alone requires managed session IDs among built-ins.

use super::*;

// =============================================================================
// BuiltInRunnerPlugin Tests
// =============================================================================

#[test]
fn all_built_in_plugins_have_correct_runner_mapping() {
    assert_eq!(BuiltInRunnerPlugin::Codex.runner(), Runner::Codex);
    assert_eq!(BuiltInRunnerPlugin::Opencode.runner(), Runner::Opencode);
    assert_eq!(BuiltInRunnerPlugin::Gemini.runner(), Runner::Gemini);
    assert_eq!(BuiltInRunnerPlugin::Claude.runner(), Runner::Claude);
    assert_eq!(BuiltInRunnerPlugin::Kimi.runner(), Runner::Kimi);
    assert_eq!(BuiltInRunnerPlugin::Pi.runner(), Runner::Pi);
    assert_eq!(BuiltInRunnerPlugin::Cursor.runner(), Runner::Cursor);
}

#[test]
fn all_built_in_plugins_have_correct_id() {
    assert_eq!(BuiltInRunnerPlugin::Codex.id(), "codex");
    assert_eq!(BuiltInRunnerPlugin::Opencode.id(), "opencode");
    assert_eq!(BuiltInRunnerPlugin::Gemini.id(), "gemini");
    assert_eq!(BuiltInRunnerPlugin::Claude.id(), "claude");
    assert_eq!(BuiltInRunnerPlugin::Kimi.id(), "kimi");
    assert_eq!(BuiltInRunnerPlugin::Pi.id(), "pi");
    assert_eq!(BuiltInRunnerPlugin::Cursor.id(), "cursor");
}

#[test]
fn all_built_in_plugins_have_metadata() {
    let plugins: [BuiltInRunnerPlugin; 7] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Opencode,
        BuiltInRunnerPlugin::Gemini,
        BuiltInRunnerPlugin::Claude,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Pi,
        BuiltInRunnerPlugin::Cursor,
    ];

    for plugin in &plugins {
        let metadata: RunnerMetadata = plugin.metadata();
        assert!(
            !metadata.id.is_empty(),
            "Plugin {:?} missing id",
            plugin.runner()
        );
        assert!(
            !metadata.name.is_empty(),
            "Plugin {:?} missing name",
            plugin.runner()
        );
        assert_eq!(metadata.id, plugin.id());
    }
}

#[test]
fn all_built_in_plugins_support_resume() {
    let plugins: [BuiltInRunnerPlugin; 7] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Opencode,
        BuiltInRunnerPlugin::Gemini,
        BuiltInRunnerPlugin::Claude,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Pi,
        BuiltInRunnerPlugin::Cursor,
    ];

    for plugin in &plugins {
        let metadata: RunnerMetadata = plugin.metadata();
        assert!(
            metadata.supports_resume,
            "Plugin {:?} should support resume",
            plugin.runner()
        );
    }
}

#[test]
fn kimi_requires_managed_session_id() {
    assert!(BuiltInRunnerPlugin::Kimi.requires_managed_session_id());
}

#[test]
fn other_plugins_do_not_require_managed_session_id() {
    assert!(!BuiltInRunnerPlugin::Codex.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Opencode.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Gemini.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Claude.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Pi.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Cursor.requires_managed_session_id());
}
