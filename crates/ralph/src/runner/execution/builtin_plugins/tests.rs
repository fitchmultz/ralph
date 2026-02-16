//! Tests for built-in runner plugins.
//!
//! Responsibilities:
//! - Validate plugin metadata and capabilities.
//! - Test response parsers for each runner format.
//! - Test helper functions.

use std::path::Path;

use super::pi::pi_session_dir_name;
use super::*;
use crate::runner::execution::plugin_trait::ResponseParser;

#[test]
fn all_built_in_plugins_have_metadata() {
    let plugins = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Opencode,
        BuiltInRunnerPlugin::Gemini,
        BuiltInRunnerPlugin::Claude,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Pi,
        BuiltInRunnerPlugin::Cursor,
    ];

    for plugin in &plugins {
        let metadata = plugin.metadata();
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
fn kimi_requires_managed_session_id() {
    assert!(BuiltInRunnerPlugin::Kimi.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Codex.requires_managed_session_id());
    assert!(!BuiltInRunnerPlugin::Claude.requires_managed_session_id());
}

#[test]
fn codex_response_parser_extracts_agent_message() {
    let parser = CodexResponseParser;
    let mut buffer = String::new();

    let json = serde_json::json!({
        "type": "item.completed",
        "item": {
            "type": "agent_message",
            "text": "Hello world"
        }
    });
    let result = parser.parse(&json, &mut buffer);

    assert_eq!(result, Some("Hello world".to_string()));
}

#[test]
fn kimi_response_parser_extracts_assistant_text() {
    let parser = KimiResponseParser;
    let mut buffer = String::new();

    let json = serde_json::json!({
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello from Kimi"}]
    });
    let result = parser.parse(&json, &mut buffer);

    assert_eq!(result, Some("Hello from Kimi".to_string()));
}

#[test]
fn pi_session_dir_name_normalizes_path() {
    let name = pi_session_dir_name(Path::new("/Users/mitchfultz/Projects/AI/ralph"));
    assert_eq!(name, "--Users-mitchfultz-Projects-AI-ralph--");
}

#[test]
fn extract_text_content_handles_string() {
    let json = serde_json::Value::String("  hello world  ".to_string());
    assert_eq!(extract_text_content(&json), Some("hello world".to_string()));
}

#[test]
fn extract_text_content_handles_array() {
    let json = serde_json::Value::Array(vec![
        serde_json::json!({"type": "text", "text": "line 1"}),
        serde_json::json!({"type": "text", "text": "line 2"}),
    ]);
    assert_eq!(
        extract_text_content(&json),
        Some("line 1\nline 2".to_string())
    );
}
