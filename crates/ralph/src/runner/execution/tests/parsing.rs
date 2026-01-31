//! Parsing tests for runner execution helpers.

use super::super::command::{effort_as_str, permission_mode_to_arg};
use super::super::json::{
    extract_session_id_from_json, extract_session_id_from_text, parse_json_line,
};
use crate::contracts::{ClaudePermissionMode, ReasoningEffort};
use serde_json::json;

#[test]
fn permission_mode_to_arg_mapping() {
    assert_eq!(
        permission_mode_to_arg(ClaudePermissionMode::AcceptEdits),
        "acceptEdits"
    );
    assert_eq!(
        permission_mode_to_arg(ClaudePermissionMode::BypassPermissions),
        "bypassPermissions"
    );
}

#[test]
fn effort_as_str_mapping() {
    assert_eq!(effort_as_str(ReasoningEffort::Low), "low");
    assert_eq!(effort_as_str(ReasoningEffort::Medium), "medium");
    assert_eq!(effort_as_str(ReasoningEffort::High), "high");
    assert_eq!(effort_as_str(ReasoningEffort::XHigh), "xhigh");
}

#[test]
fn parse_json_line_handles_invalid_json() {
    assert!(parse_json_line("{").is_none());
}

#[test]
fn parse_json_line_parses_json_with_prefix_noise() {
    let line = "[INFO] {\"type\":\"assistant\",\"session_id\":\"sess-001\"} trailing";
    let json = parse_json_line(line).expect("should parse");
    assert_eq!(
        json.get("session_id").and_then(|v| v.as_str()),
        Some("sess-001")
    );
}

#[test]
fn extract_session_id_from_json_codex_thread_id() {
    let payload = json!({
        "thread_id": "thread-123"
    });
    assert_eq!(
        extract_session_id_from_json(&payload),
        Some("thread-123".to_string())
    );
}

#[test]
fn extract_session_id_from_json_claude_session_id() {
    let payload = json!({
        "session_id": "session-abc"
    });
    assert_eq!(
        extract_session_id_from_json(&payload),
        Some("session-abc".to_string())
    );
}

#[test]
fn extract_session_id_from_json_gemini_session_id() {
    let payload = json!({
        "session_id": "gemini-xyz"
    });
    assert_eq!(
        extract_session_id_from_json(&payload),
        Some("gemini-xyz".to_string())
    );
}

#[test]
fn extract_session_id_from_json_opencode_session_id() {
    let payload = json!({
        "sessionID": "open-789"
    });
    assert_eq!(
        extract_session_id_from_json(&payload),
        Some("open-789".to_string())
    );
}

#[test]
fn extract_session_id_from_json_pi_session_event() {
    let payload = json!({
        "type": "session",
        "id": "pi-123"
    });
    assert_eq!(
        extract_session_id_from_json(&payload),
        Some("pi-123".to_string())
    );
}

#[test]
fn extract_session_id_from_text_reads_json_lines() {
    let stdout = "{\"session_id\":\"sess-001\"}\n{\"result\":\"ok\"}\n";
    assert_eq!(
        extract_session_id_from_text(stdout),
        Some("sess-001".to_string())
    );
}

#[test]
fn extract_session_id_with_prefix() {
    let stdout = "[INFO] {\"session_id\":\"sess-with-prefix\"}\n";
    assert_eq!(
        extract_session_id_from_text(stdout),
        Some("sess-with-prefix".to_string())
    );
}

#[test]
fn extract_session_id_with_suffix() {
    let stdout = "{\"session_id\":\"sess-with-suffix\"} [OK]\n";
    assert_eq!(
        extract_session_id_from_text(stdout),
        Some("sess-with-suffix".to_string())
    );
}

#[test]
fn extract_session_id_interleaved_garbage() {
    let stdout = "Starting runner...\n[DEBUG] init\n{\"session_id\":\"sess-interleaved\"}\nDone.\n";
    assert_eq!(
        extract_session_id_from_text(stdout),
        Some("sess-interleaved".to_string())
    );
}

#[test]
fn extract_session_id_non_string_values() {
    // Should skip numeric ID if strict string is expected, or just fail gracefully (return None)
    // The current implementation checks .as_str(), so it should return None for this line,
    // and if there's no other valid line, return None overall.
    let stdout = "{\"session_id\":12345}\n";
    assert_eq!(extract_session_id_from_text(stdout), None);
}

#[test]
fn extract_session_id_nested_fields_ignored() {
    // Ensure we don't pick up nested session_ids if we only look at top level (current impl uses from_str -> Value, checks top level)
    let stdout = "{\"data\": {\"session_id\":\"nested-id\"}}\n";
    assert_eq!(extract_session_id_from_text(stdout), None);
}

#[test]
fn extract_session_id_from_json_kimi_tool_calls() {
    let payload = json!({
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello"}],
        "tool_calls": [
            {"type": "function", "id": "tool_bUJW2GCXzg65VTa72XV9YhNn", "function": {"name": "test"}}
        ]
    });
    assert_eq!(
        extract_session_id_from_json(&payload),
        Some("tool_bUJW2GCXzg65VTa72XV9YhNn".to_string())
    );
}

#[test]
fn extract_session_id_from_json_kimi_no_tool_calls() {
    let payload = json!({
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello"}]
    });
    assert_eq!(extract_session_id_from_json(&payload), None);
}

#[test]
fn extract_session_id_from_json_kimi_empty_tool_calls() {
    let payload = json!({
        "role": "assistant",
        "tool_calls": []
    });
    assert_eq!(extract_session_id_from_json(&payload), None);
}

#[test]
fn extract_session_id_from_json_kimi_tool_without_id() {
    let payload = json!({
        "role": "assistant",
        "tool_calls": [
            {"type": "function", "function": {"name": "test"}}
        ]
    });
    assert_eq!(extract_session_id_from_json(&payload), None);
}

#[test]
fn extract_session_id_from_text_kimi_format() {
    let stdout = r#"{"role":"assistant","content":[{"type":"text","text":"Hello"}],"tool_calls":[{"id":"tool_xyz789","type":"function"}]}"#;
    assert_eq!(
        extract_session_id_from_text(stdout),
        Some("tool_xyz789".to_string())
    );
}
