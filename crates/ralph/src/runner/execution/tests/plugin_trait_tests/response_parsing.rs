//! Plugin response-parser regression coverage.
//!
//! Purpose:
//! - Plugin response-parser regression coverage.
//!
//! Responsibilities:
//! - Verify built-in parser extraction across valid, invalid, and empty response lines.
//! - Preserve multiline accumulation behavior where runner protocols require it.
//!
//! Non-scope:
//! - Command construction or executor dispatch metadata.
//! - Live subprocess streaming integration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Parsers should return `None` for invalid JSON rather than panic.
//! - Streaming runners may accumulate buffer state across successive lines.

use super::*;

// =============================================================================

#[test]
fn codex_response_parser_extracts_agent_message() {
    let plugin = BuiltInRunnerPlugin::Codex;
    let mut buffer = String::new();

    let line =
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Hello from Codex"}}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Codex".to_string()));
}

#[test]
fn kimi_response_parser_extracts_assistant_text() {
    let plugin = BuiltInRunnerPlugin::Kimi;
    let mut buffer = String::new();

    let line = r#"{"role":"assistant","content":[{"type":"text","text":"Hello from Kimi"}]}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Kimi".to_string()));
}

#[test]
fn kimi_response_parser_skips_non_assistant_role() {
    let plugin = BuiltInRunnerPlugin::Kimi;
    let mut buffer = String::new();

    let line = r#"{"role":"user","content":[{"type":"text","text":"User message"}]}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, None);
}

#[test]
fn claude_response_parser_extracts_assistant_message() {
    let plugin = BuiltInRunnerPlugin::Claude;
    let mut buffer = String::new();

    let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello from Claude"}]}}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Claude".to_string()));
}

#[test]
fn gemini_response_parser_extracts_message() {
    let plugin = BuiltInRunnerPlugin::Gemini;
    let mut buffer = String::new();

    let line = r#"{"type":"message","role":"assistant","content":"Hello from Gemini"}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Gemini".to_string()));
}

#[test]
fn opencode_response_parser_accumulates_streaming_text() {
    let plugin = BuiltInRunnerPlugin::Opencode;
    let mut buffer = String::new();

    let line1 = r#"{"type":"text","part":{"text":"Hello "}}"#;
    let line2 = r#"{"type":"text","part":{"text":"World"}}"#;

    let result1: Option<String> = plugin.parse_response_line(line1, &mut buffer);
    assert_eq!(result1, Some("Hello ".to_string()));

    let result2: Option<String> = plugin.parse_response_line(line2, &mut buffer);
    assert_eq!(result2, Some("Hello World".to_string()));
}

#[test]
fn cursor_response_parser_extracts_message_end() {
    let plugin = BuiltInRunnerPlugin::Cursor;
    let mut buffer = String::new();

    let line =
        r#"{"type":"message_end","message":{"role":"assistant","content":"Hello from Cursor"}}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Cursor".to_string()));
}

#[test]
fn pi_response_parser_extracts_result() {
    let plugin = BuiltInRunnerPlugin::Pi;
    let mut buffer = String::new();

    let line = r#"{"type":"result","result":"Hello from Pi"}"#;
    let result: Option<String> = plugin.parse_response_line(line, &mut buffer);

    assert_eq!(result, Some("Hello from Pi".to_string()));
}

#[test]
fn response_parsers_handle_invalid_json() {
    let plugins: [BuiltInRunnerPlugin; 7] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Claude,
        BuiltInRunnerPlugin::Gemini,
        BuiltInRunnerPlugin::Opencode,
        BuiltInRunnerPlugin::Cursor,
        BuiltInRunnerPlugin::Pi,
    ];

    for plugin in &plugins {
        let mut buffer = String::new();
        let result: Option<String> = plugin.parse_response_line("not valid json", &mut buffer);
        assert_eq!(
            result,
            None,
            "Plugin {:?} should return None for invalid JSON",
            plugin.runner()
        );
    }
}

#[test]
fn response_parsers_handle_empty_lines() {
    let plugins: [BuiltInRunnerPlugin; 3] = [
        BuiltInRunnerPlugin::Codex,
        BuiltInRunnerPlugin::Kimi,
        BuiltInRunnerPlugin::Claude,
    ];

    for plugin in &plugins {
        let mut buffer = String::new();
        let result: Option<String> = plugin.parse_response_line("", &mut buffer);
        assert_eq!(
            result,
            None,
            "Plugin {:?} should return None for empty lines",
            plugin.runner()
        );
    }
}
