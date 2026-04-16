//! Stream display filtering regression coverage.
//!
//! Responsibilities:
//! - Verify display-line extraction across supported runner stream payloads.
//! - Cover handler fanout for filtered JSON output.
//!
//! Does not handle:
//! - Reader thread buffer limits and EOF behavior.
//! - Broader execution orchestration outside display formatting.
//!
//! Assumptions/invariants:
//! - Display extraction should stay no-op for unknown events.
//! - Tool result rendering must avoid leaking verbose tool payload bodies.

use super::*;

#[test]
fn extract_display_lines_codex_agent_message() {
    let payload = json!({
        "type": "item.completed",
        "item": {"type": "agent_message", "text": "Hi!"}
    });
    assert_eq!(extract_display_lines(&payload), vec!["Hi!", ""]);
}

#[test]
fn extract_display_lines_codex_reasoning() {
    let payload = json!({
        "type": "item.completed",
        "item": {"type": "reasoning", "text": "Working it out"}
    });
    let lines = extract_display_lines(&payload);
    assert_eq!(lines.len(), 1);

    // Line should contain [Reasoning] prefix and the content text
    // Note: format_reasoning() adds ANSI color codes to the prefix only
    let reasoning_line = &lines[0];
    assert!(reasoning_line.contains("[Reasoning]"));
    assert!(reasoning_line.contains("Working it out"));
}

#[test]
fn extract_display_lines_codex_tool_call() {
    let payload = json!({
        "type": "item.completed",
        "item": {
            "type": "mcp_tool_call",
            "server": "RepoPrompt",
            "tool": "get_file_tree",
            "status": "completed",
            "arguments": {
                "path": "/tmp/project",
                "pattern": "*.rs"
            }
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] RepoPrompt.get_file_tree (completed) path=/tmp/project pattern=*.rs"]
    );
}

#[test]
fn extract_display_lines_codex_command_execution() {
    let payload = json!({
        "type": "item.started",
        "item": {
            "type": "command_execution",
            "command": "/bin/zsh -lc ls",
            "status": "in_progress",
            "exit_code": null
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Command] /bin/zsh -lc ls (in_progress)"]
    );
}

#[test]
fn extract_display_lines_claude_result_and_tool_use() {
    let payload = json!({
        "result": "Final answer",
        "type": "assistant",
        "message": {
            "content": [
                {"type": "text", "text": "Streamed text"},
                {"type": "tool_use", "name": "Read", "input": {"file_path": "/tmp/a.txt"}}
            ]
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec![
            "Final answer",
            "Streamed text",
            "[Tool] Read path=/tmp/a.txt"
        ]
    );
}

#[test]
fn extract_display_lines_permission_denial() {
    let payload = json!({
        "permission_denials": [
            {"tool_name": "write"}
        ]
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Permission denied: write]"]
    );
}

#[test]
fn display_filtered_json_calls_output_handler() {
    let payload = json!({
        "type": "text",
        "part": { "text": "hello" }
    });
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let handler: OutputHandler = Arc::new(Box::new({
        let captured = Arc::clone(&captured);
        move |text: &str| {
            captured
                .lock()
                .expect("capture lock")
                .push(text.to_string());
        }
    }));

    display_filtered_json(
        &payload,
        &StreamSink::Stdout,
        Some(&handler),
        OutputStream::HandlerOnly,
    )
    .expect("display filtered json");

    let guard = captured.lock().expect("capture lock");
    assert_eq!(guard.as_slice(), &["hello\n".to_string()]);
}

#[test]
fn extract_display_lines_opencode_text() {
    let payload = json!({
        "type": "text",
        "part": { "text": "hello" }
    });
    assert_eq!(extract_display_lines(&payload), vec!["hello"]);
}

#[test]
fn extract_display_lines_opencode_reasoning() {
    let payload = json!({
        "type": "reasoning",
        "part": { "text": "Considering tool strategy" }
    });
    let lines = extract_display_lines(&payload);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("[Reasoning]"));
    assert!(lines[0].contains("Considering tool strategy"));
}

#[test]
fn extract_display_lines_opencode_tool_use() {
    let payload = json!({
        "type": "tool_use",
        "part": {
            "tool": "read",
            "state": {
                "status": "completed",
                "input": { "filePath": "/tmp/example.txt" }
            }
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] read (completed) path=/tmp/example.txt"]
    );
}

#[test]
fn extract_display_lines_gemini_tool_use_and_result() {
    let tool_use = json!({
        "type": "tool_use",
        "tool_name": "read_file",
        "parameters": { "file_path": "notes.txt" }
    });
    assert_eq!(
        extract_display_lines(&tool_use),
        vec!["[Tool] read_file path=notes.txt"]
    );

    let tool_result = json!({
        "type": "tool_result",
        "tool_name": "read_file",
        "status": "success"
    });
    assert_eq!(
        extract_display_lines(&tool_result),
        vec!["[Tool] read_file (success)"]
    );
}

#[test]
fn extract_display_lines_gemini_message_assistant() {
    let payload = json!({
        "type": "message",
        "role": "assistant",
        "content": "hi"
    });
    assert_eq!(extract_display_lines(&payload), vec!["hi"]);
}

#[test]
fn extract_display_lines_claude_result_error_payload() {
    let payload = json!({
        "type": "result",
        "is_error": true,
        "errors": ["invalid session id"]
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Error] invalid session id"]
    );
}

#[test]
fn extract_display_lines_codex_error_event() {
    let payload = json!({
        "type": "error",
        "message": "Session stream failed"
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Error] Session stream failed"]
    );
}

#[test]
fn extract_display_lines_unknown_event_is_noop() {
    let payload = json!({"type": "unknown"});
    assert!(extract_display_lines(&payload).is_empty());
}

#[test]
fn extract_display_lines_kimi_assistant_with_think() {
    let payload = json!({
        "role": "assistant",
        "content": [
            {"type": "think", "think": "Analyzing the request"},
            {"type": "text", "text": "Hello! How can I help?"}
        ]
    });
    let lines = extract_display_lines(&payload);
    assert_eq!(lines.len(), 2);

    // First line should contain [Reasoning] prefix and the reasoning content
    // Note: format_reasoning() adds ANSI color codes to the prefix only
    let reasoning_line = &lines[0];
    assert!(reasoning_line.contains("[Reasoning]"));
    assert!(reasoning_line.contains("Analyzing the request"));

    // Second line is plain text content
    assert_eq!(lines[1], "Hello! How can I help?");
}

#[test]
fn extract_display_lines_kimi_assistant_text_only() {
    let payload = json!({
        "role": "assistant",
        "content": [
            {"type": "text", "text": "Response text"}
        ]
    });
    assert_eq!(extract_display_lines(&payload), vec!["Response text"]);
}

#[test]
fn extract_display_lines_kimi_with_tool_calls() {
    let payload = json!({
        "role": "assistant",
        "content": [{"type": "text", "text": "Using tool"}],
        "tool_calls": [{"id": "tool_abc123", "type": "function"}]
    });
    // Should extract text content, ignore tool_calls for display
    assert_eq!(extract_display_lines(&payload), vec!["Using tool"]);
}

#[test]
fn extract_display_lines_kimi_empty_content() {
    let payload = json!({
        "role": "assistant",
        "content": []
    });
    assert!(extract_display_lines(&payload).is_empty());
}

#[test]
fn extract_display_lines_kimi_no_role_field() {
    // Without role="assistant", should not be treated as kimi format
    let payload = json!({
        "content": [{"type": "text", "text": "Some text"}]
    });
    assert!(extract_display_lines(&payload).is_empty());
}

#[test]
fn extract_display_lines_pi_message_end_assistant() {
    let payload = json!({
        "type": "message_end",
        "message": {
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Key findings cited in evidence: Alpha"}
            ]
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["Key findings cited in evidence: Alpha"]
    );
}

#[test]
fn extract_display_lines_pi_message_end_tool_result() {
    let payload = json!({
        "type": "message_end",
        "message": {
            "role": "toolResult",
            "toolName": "bash",
            "isError": false
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] bash (completed)"]
    );
}

#[test]
fn extract_display_lines_pi_message_end_tool_result_without_tool_name() {
    let payload = json!({
        "type": "message_end",
        "message": {
            "role": "toolResult",
            "isError": false
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] Tool (completed)"]
    );
}

#[test]
fn extract_display_lines_pi_message_update_thinking_end() {
    let payload = json!({
        "type": "message_update",
        "assistantMessageEvent": {
            "type": "thinking_end",
            "content": "Inspecting the repository layout"
        }
    });
    let lines = extract_display_lines(&payload);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("[Reasoning]"));
    assert!(lines[0].contains("Inspecting the repository layout"));
}

#[test]
fn extract_display_lines_pi_tool_execution_start_bash() {
    let payload = json!({
        "type": "tool_execution_start",
        "toolName": "bash",
        "args": {
            "command": "git status --short",
            "timeout": 20
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] bash cmd=git status --short"]
    );
}

#[test]
fn extract_display_lines_pi_tool_execution_start_read() {
    let payload = json!({
        "type": "tool_execution_start",
        "toolName": "read",
        "args": {
            "path": "src/main.rs"
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] read path=src/main.rs"]
    );
}

#[test]
fn extract_display_lines_cursor_tool_call_mcp() {
    let payload = json!({
        "type": "tool_call",
        "tool_call": {
            "mcpToolCall": {
                "args": {
                    "providerIdentifier": "RepoPrompt",
                    "toolName": "list_windows",
                    "args": {}
                }
            }
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] RepoPrompt.list_windows"]
    );
}

#[test]
fn extract_display_lines_cursor_tool_call_shell() {
    let payload = json!({
        "type": "tool_call",
        "tool_call": {
            "shellToolCall": {
                "args": {
                    "command": "ls -la"
                }
            }
        }
    });
    assert_eq!(
        extract_display_lines(&payload),
        vec!["[Tool] shell cmd=ls -la"]
    );
}

#[test]
fn extract_display_lines_kimi_tool_result_shows_completed() {
    // Tool results show "(completed)" rather than content to avoid verbose output
    let payload = json!({
        "role": "tool",
        "content": [{"type": "text", "text": "file contents here"}],
        "tool_call_id": "call_1",
        "tool_name": "read_file"
    });
    let result = extract_display_lines(&payload);
    assert_eq!(result.len(), 1);
    // format_tool_call produces "[Tool] tool_name (completed)" format
    assert!(result[0].contains("[Tool]"));
    assert!(result[0].contains("read_file"));
    assert!(result[0].contains("(completed)"));
    // Should NOT contain the actual content
    assert!(!result[0].contains("file contents here"));
}

#[test]
fn extract_display_lines_kimi_tool_result_without_tool_name() {
    // Without tool_name, defaults to "Tool"
    let payload = json!({
        "role": "tool",
        "content": [{"type": "text", "text": "output data"}],
        "tool_call_id": "call_1"
    });
    let result = extract_display_lines(&payload);
    assert_eq!(result.len(), 1);
    // format_tool_call produces "[Tool] Tool (completed)" format when tool_name is missing
    assert!(result[0].contains("[Tool]"));
    assert!(result[0].contains("Tool"));
    assert!(result[0].contains("(completed)"));
}

#[test]
fn extract_display_lines_kimi_tool_result_empty_content() {
    // Empty content still shows (completed)
    let payload = json!({
        "role": "tool",
        "content": [],
        "tool_call_id": "call_1",
        "tool_name": "search"
    });
    let result = extract_display_lines(&payload);
    assert_eq!(result.len(), 1);
    // format_tool_call produces "[Tool] tool_name (completed)" format
    assert!(result[0].contains("[Tool]"));
    assert!(result[0].contains("search"));
    assert!(result[0].contains("(completed)"));
}

#[test]
fn extract_display_lines_kimi_tool_result_no_content_field() {
    // No content field still shows (completed)
    let payload = json!({
        "role": "tool",
        "tool_call_id": "call_1",
        "tool_name": "list_files"
    });
    let result = extract_display_lines(&payload);
    assert_eq!(result.len(), 1);
    // format_tool_call produces "[Tool] tool_name (completed)" format
    assert!(result[0].contains("[Tool]"));
    assert!(result[0].contains("list_files"));
    assert!(result[0].contains("(completed)"));
}

#[test]
fn extract_display_lines_kimi_assistant_with_mixed_content() {
    // Test assistant message with both text and think parts
    let payload = json!({
        "role": "assistant",
        "content": [
            {"type": "think", "think": "Let me analyze this"},
            {"type": "text", "text": "Here's the answer"}
        ]
    });
    let result = extract_display_lines(&payload);
    assert_eq!(result.len(), 2);
    assert!(result[0].contains("Let me analyze this"));
    assert_eq!(result[1], "Here's the answer");
}

#[test]
fn extract_display_lines_kimi_tool_result_multiple_content_parts() {
    // Multiple content parts still just show single (completed) line
    let payload = json!({
        "role": "tool",
        "content": [
            {"type": "text", "text": "First part"},
            {"type": "text", "text": "Second part"}
        ],
        "tool_call_id": "call_1",
        "tool_name": "bash"
    });
    let result = extract_display_lines(&payload);
    assert_eq!(result.len(), 1);
    // format_tool_call produces "[Tool] tool_name (completed)" format
    assert!(result[0].contains("[Tool]"));
    assert!(result[0].contains("bash"));
    assert!(result[0].contains("(completed)"));
}
