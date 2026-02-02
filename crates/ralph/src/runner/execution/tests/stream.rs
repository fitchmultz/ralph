//! Stream filtering tests for runner execution output.

use super::super::stream::{StreamSink, display_filtered_json, extract_display_lines};
use crate::constants::buffers::{MAX_BUFFER_SIZE, MAX_LINE_LENGTH};
use crate::runner::{OutputHandler, OutputStream};
use serde_json::json;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

// Re-export spawn functions for testing (they're pub(super) in parent)
use super::super::stream::{spawn_json_reader, spawn_reader};

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
fn max_line_length_constant_is_10mb() {
    // Verify the constant is set to expected 10MB value
    assert_eq!(MAX_LINE_LENGTH, 10 * 1024 * 1024);
}

#[test]
fn max_buffer_size_constant_is_10mb() {
    // Verify the constant is set to expected 10MB value
    assert_eq!(MAX_BUFFER_SIZE, 10 * 1024 * 1024);
}

#[test]
fn spawn_json_reader_handles_normal_lines() {
    let input = r#"{"type":"text","part":{"text":"hello world"}}"#;
    let reader = Cursor::new(input.as_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.contains("hello world"));
}

#[test]
fn spawn_json_reader_enforces_line_length_limit() {
    // Create input that exceeds MAX_LINE_LENGTH without newlines
    // Use owned data to satisfy 'static requirement
    let oversized_data: Vec<u8> = vec![b'x'; MAX_LINE_LENGTH + 1000];
    let reader = Cursor::new(oversized_data);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    // The shared output buffer should not grow beyond MAX_BUFFER_SIZE.
    // Note: line_buf has MAX_LINE_LENGTH protection, but the shared buffer
    // has MAX_BUFFER_SIZE protection (both are 10MB in current config).
    let guard = buffer.lock().unwrap();
    assert!(guard.len() <= MAX_BUFFER_SIZE);
}

#[test]
fn spawn_json_reader_handles_multiple_lines_within_limit() {
    // Create multiple normal-sized lines
    let lines: Vec<String> = (0..100)
        .map(|i| format!(r#"{{"type":"text","part":{{"text":"line {}"}}}}"#, i))
        .collect();
    let input = lines.join("\n");
    // Use owned data to satisfy 'static requirement
    let reader = Cursor::new(input.into_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    // Buffer should contain all the input since lines are processed and cleared
    assert!(guard.contains("line 0"));
    assert!(guard.contains("line 99"));
}

#[test]
fn spawn_reader_enforces_buffer_limit() {
    // Create input that exceeds MAX_BUFFER_SIZE
    // Use owned data to satisfy 'static requirement
    let oversized_data: Vec<u8> = vec![b'x'; MAX_BUFFER_SIZE + 10000];
    let reader = Cursor::new(oversized_data);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let handle = spawn_reader(
        reader,
        StreamSink::Stderr,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    // The buffer should be truncated to MAX_BUFFER_SIZE
    let guard = buffer.lock().unwrap();
    assert!(guard.len() <= MAX_BUFFER_SIZE);
}

#[test]
fn spawn_reader_handles_normal_output() {
    let input = "Hello, world!\nThis is a test.\n";
    let reader = Cursor::new(input.as_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let handle = spawn_reader(
        reader,
        StreamSink::Stderr,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert_eq!(guard.as_str(), input);
}

#[test]
fn spawn_json_reader_handles_empty_input() {
    let reader = Cursor::new(b"");
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.is_empty());
}

#[test]
fn spawn_reader_handles_empty_input() {
    let reader = Cursor::new(b"");
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let handle = spawn_reader(
        reader,
        StreamSink::Stderr,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.is_empty());
}

#[test]
fn spawn_json_reader_handles_line_exactly_at_limit() {
    // Create a line that is exactly at MAX_LINE_LENGTH
    // Use owned data to satisfy 'static requirement
    let exact_size_data: Vec<u8> = vec![b'x'; MAX_LINE_LENGTH];
    let reader = Cursor::new(exact_size_data);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    // The buffer should contain exactly MAX_LINE_LENGTH characters
    let guard = buffer.lock().unwrap();
    assert_eq!(guard.len(), MAX_LINE_LENGTH);
}

#[test]
fn spawn_json_reader_handles_partial_line_at_eof() {
    // Create a partial line (no trailing newline) that should still be processed
    let partial_line = r#"{"type":"text","part":{"text":"partial"}}"#;
    let reader = Cursor::new(partial_line.as_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.contains("partial"));
}
