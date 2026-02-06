//! Response extraction tests for runner outputs.

use super::super::extract_final_assistant_response;

#[test]
fn extract_final_assistant_response_codex_agent_message() {
    let stdout = concat!(
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Draft"}}"#,
        "\n",
        r#"{"type":"item.completed","item":{"type":"agent_message","text":"Final answer"}}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Final answer".to_string())
    );
}

#[test]
fn extract_final_assistant_response_claude_assistant_message() {
    let stdout = concat!(
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"First line"},{"type":"tool_use","name":"Read"}]}}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("First line".to_string())
    );
}

#[test]
fn extract_final_assistant_response_gemini_message_assistant() {
    let stdout = concat!(
        r#"{"type":"message","role":"assistant","content":[{"text":"Hello"},{"text":"World"}]}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Hello\nWorld".to_string())
    );
}

#[test]
fn extract_final_assistant_response_opencode_text_stream() {
    let stdout = concat!(
        r#"{"type":"text","part":{"text":"Hello "}}"#,
        "\n",
        r#"{"type":"text","part":{"text":"world"}}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Hello world".to_string())
    );
}

#[test]
fn extract_final_assistant_response_none_when_missing() {
    let stdout = concat!(r#"{"type":"tool_use","tool_name":"read"}"#, "\n");
    assert_eq!(extract_final_assistant_response(stdout), None);
}

#[test]
fn extract_final_assistant_response_kimi_assistant() {
    let stdout = concat!(
        r#"{"role":"assistant","content":[{"type":"think","think":"Reasoning"},{"type":"text","text":"Final response"}]}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Final response".to_string())
    );
}

#[test]
fn extract_final_assistant_response_kimi_multiple_text() {
    let stdout = concat!(
        r#"{"role":"assistant","content":[{"type":"text","text":"Line 1"},{"type":"text","text":"Line 2"}]}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Line 1\nLine 2".to_string())
    );
}

#[test]
fn extract_final_assistant_response_kimi_no_text_content() {
    let stdout = concat!(
        r#"{"role":"assistant","content":[{"type":"think","think":"Just thinking"}]}"#,
        "\n"
    );
    assert_eq!(extract_final_assistant_response(stdout), None);
}

#[test]
fn extract_final_assistant_response_kimi_no_role() {
    // Without role="assistant", should not match kimi format
    let stdout = concat!(r#"{"content":[{"type":"text","text":"Some text"}]}"#, "\n");
    assert_eq!(extract_final_assistant_response(stdout), None);
}

#[test]
fn extract_final_assistant_response_kimi_empty_content() {
    let stdout = concat!(r#"{"role":"assistant","content":[]}"#, "\n");
    assert_eq!(extract_final_assistant_response(stdout), None);
}

#[test]
fn extract_final_assistant_response_cursor_result() {
    let stdout = concat!(r#"{"type":"result","result":"Final cursor output"}"#, "\n");
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Final cursor output".to_string())
    );
}

#[test]
fn extract_final_assistant_response_pi_message_end() {
    let stdout = concat!(
        r#"{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"Key findings cited in evidence: Alpha"}]}}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Key findings cited in evidence: Alpha".to_string())
    );
}

#[test]
fn extract_kimi_assistant_response_with_thinking() {
    // Input: kimi JSON with both think and text content
    // Expected: Only text content extracted (thinking excluded for final response)
    let stdout = concat!(
        r#"{"role":"assistant","content":[{"type":"think","think":"Deep reasoning about the problem"},{"type":"text","text":"Final answer"}]}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Final answer".to_string())
    );
}

#[test]
fn extract_kimi_assistant_response_with_tool_calls() {
    // Input: kimi JSON with text content and tool_calls array
    // Expected: Only text content extracted (tool calls don't affect final response)
    let stdout = concat!(
        r#"{"role":"assistant","content":[{"type":"text","text":"Using tool"}],"tool_calls":[{"id":"tool_abc123","type":"function","function":{"name":"read_file"}}]}"#,
        "\n"
    );
    // Tool calls shouldn't break text extraction
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("Using tool".to_string())
    );
}

#[test]
fn extract_kimi_assistant_response_thinking_only() {
    // Input: kimi JSON with only think content, no text
    // Expected: None (no text to extract)
    let stdout = concat!(
        r#"{"role":"assistant","content":[{"type":"think","think":"Only reasoning here"}]}"#,
        "\n"
    );
    assert_eq!(extract_final_assistant_response(stdout), None);
}

#[test]
fn extract_kimi_assistant_response_empty_content() {
    // Input: kimi JSON with empty content array
    // Expected: None
    let stdout = concat!(r#"{"role":"assistant","content":[]}"#, "\n");
    assert_eq!(extract_final_assistant_response(stdout), None);
}

#[test]
fn extract_kimi_assistant_response_mixed_types() {
    // Input: kimi JSON with text, think, and unknown types
    // Expected: Only text content extracted, others ignored
    let stdout = concat!(
        r#"{"role":"assistant","content":[{"type":"think","think":"Reasoning..."},{"type":"text","text":"First part"},{"type":"unknown","data":"ignored"},{"type":"text","text":"Second part"},{"type":"tool_result","result":"also ignored"}]}"#,
        "\n"
    );
    assert_eq!(
        extract_final_assistant_response(stdout),
        Some("First part\nSecond part".to_string())
    );
}
