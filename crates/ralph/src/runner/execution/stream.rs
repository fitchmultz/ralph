//! Streaming reader and display helpers for runner output.
//!
//! This module provides buffered reading and JSON parsing for AI runner output,
//! with protection against unbounded memory growth from malicious or buggy runners.
//!
//! Responsibilities:
//! - Read stdout/stderr streams from runner processes incrementally
//! - Parse JSON lines and extract meaningful display content
//! - Buffer output with size limits (MAX_LINE_LENGTH, MAX_BUFFER_SIZE)
//! - Handle tool use/result tracking for display purposes
//!
//! Explicitly does NOT handle:
//! - Runner process lifecycle (spawning, killing) - see `super::command`
//! - Output redaction (secrets filtering) - see `crate::redaction`
//! - Debug logging - see `crate::debuglog`
//! - Session ID persistence - only extracts to a shared buffer
//!
//! Invariants/Assumptions:
//! - Readers assume UTF-8 input (uses `String::from_utf8_lossy` for invalid bytes)
//! - JSON parsing is best-effort; non-JSON lines are passed through as-is
//! - Buffer limits are enforced by truncation (oldest content dropped first)
//! - Line length limit is checked per-line; buffer size limit is checked per-read

use anyhow::Context;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::constants::buffers::{MAX_BUFFER_SIZE, MAX_LINE_LENGTH, TOOL_VALUE_MAX_LEN};
use crate::debuglog::{self, DebugStream};
use crate::outpututil;

use super::super::{OutputHandler, OutputStream};
use super::json::{extract_session_id_from_json, parse_json_line};

pub(super) enum StreamSink {
    Stdout,
    Stderr,
}

impl StreamSink {
    pub(super) fn write_all(
        &self,
        bytes: &[u8],
        output_stream: OutputStream,
    ) -> std::io::Result<()> {
        if !output_stream.streams_to_terminal() {
            return Ok(());
        }
        match self {
            StreamSink::Stdout => {
                let mut out = std::io::stdout().lock();
                out.write_all(bytes)?;
                out.flush()
            }
            StreamSink::Stderr => {
                let mut err = std::io::stderr().lock();
                err.write_all(bytes)?;
                err.flush()
            }
        }
    }
}

/// Append text to buffer, truncating older content if MAX_BUFFER_SIZE would be exceeded.
/// Returns true if truncation occurred (and it was the first time), false otherwise.
fn append_to_buffer(buffer: &mut String, text: &str, exceeded_logged: &mut bool) -> bool {
    if buffer.len() + text.len() > MAX_BUFFER_SIZE {
        let should_log = !*exceeded_logged;
        if should_log {
            log::warn!(
                "Runner output buffer exceeded {}MB limit; truncating older content",
                MAX_BUFFER_SIZE / (1024 * 1024)
            );
            *exceeded_logged = true;
        }
        // Keep only the most recent content within limit
        if text.len() >= MAX_BUFFER_SIZE {
            // New text alone exceeds limit, keep just the end of it
            buffer.clear();
            buffer.push_str(&text[text.len() - MAX_BUFFER_SIZE..]);
        } else {
            // Trim old content to make room for new text
            let keep_from = buffer.len() + text.len() - MAX_BUFFER_SIZE;
            let remaining = buffer.split_off(keep_from);
            *buffer = remaining;
            buffer.push_str(text);
        }
        should_log
    } else {
        buffer.push_str(text);
        false
    }
}

pub(super) fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> thread::JoinHandle<anyhow::Result<()>> {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut buffer_exceeded_logged = false;
        loop {
            let read = reader.read(&mut buf).context("read child output")?;
            if read == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&buf[..read]);
            debuglog::write_runner_chunk(DebugStream::Stderr, text.as_ref());
            sink.write_all(&buf[..read], output_stream)
                .context("stream child output")?;
            let mut guard = buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("lock output buffer"))?;

            // Check if adding this text would exceed the buffer limit
            if guard.len() + text.len() > MAX_BUFFER_SIZE {
                if !buffer_exceeded_logged {
                    log::warn!(
                        "Runner output buffer exceeded {}MB limit; truncating older content",
                        MAX_BUFFER_SIZE / (1024 * 1024)
                    );
                    buffer_exceeded_logged = true;
                }
                // Keep only the most recent content within limit
                let text_str = text.as_ref();
                if text_str.len() >= MAX_BUFFER_SIZE {
                    // New text alone exceeds limit, keep just the end of it
                    guard.clear();
                    guard.push_str(&text_str[text_str.len() - MAX_BUFFER_SIZE..]);
                } else {
                    // Trim old content to make room for new text
                    let keep_from = guard.len() + text_str.len() - MAX_BUFFER_SIZE;
                    let remaining = guard.split_off(keep_from);
                    *guard = remaining;
                    guard.push_str(text_str);
                }
            } else {
                guard.push_str(&text);
            }

            if let Some(handler) = &output_handler {
                handler(&text);
            }
        }
        Ok(())
    })
}

/// Spawn a reader that parses JSON lines and displays meaningful content.
pub(super) fn spawn_json_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    session_id_buf: Arc<Mutex<Option<String>>>,
) -> thread::JoinHandle<anyhow::Result<()>> {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut line_buf = String::new();
        let mut line_length_exceeded = false;
        let mut buffer_exceeded_logged = false;
        let mut tool_name_by_id: HashMap<String, String> = HashMap::new();

        loop {
            let read = reader.read(&mut buf).context("read child output")?;
            if read == 0 {
                break;
            }

            let text = String::from_utf8_lossy(&buf[..read]);
            debuglog::write_runner_chunk(DebugStream::Stdout, text.as_ref());
            for ch in text.chars() {
                if ch == '\n' {
                    if line_length_exceeded {
                        // Log warning and reset the flag
                        log::warn!(
                            "Runner output line exceeded {}MB limit; truncating",
                            MAX_LINE_LENGTH / (1024 * 1024)
                        );
                        line_length_exceeded = false;
                    }
                    if let Some(mut json) = parse_json_line(&line_buf) {
                        if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
                            if event_type == "tool_use" {
                                if let (Some(tool_id), Some(tool_name)) = (
                                    json.get("tool_id").and_then(|v| v.as_str()),
                                    json.get("tool_name").and_then(|v| v.as_str()),
                                ) {
                                    tool_name_by_id
                                        .insert(tool_id.to_string(), tool_name.to_string());
                                }
                            } else if event_type == "tool_result" {
                                let tool_id = json.get("tool_id").and_then(|v| v.as_str());
                                if let Some(tool_id) = tool_id
                                    && let Some(tool_name) = tool_name_by_id.remove(tool_id)
                                    && let Some(obj) = json.as_object_mut()
                                {
                                    obj.insert(
                                        "tool_name".to_string(),
                                        JsonValue::String(tool_name),
                                    );
                                }
                            }
                        }
                        // Track kimi tool calls: assistant with tool_calls array
                        if let Some(role) = json.get("role").and_then(|r| r.as_str())
                            && role == "assistant"
                            && let Some(tool_calls) =
                                json.get("tool_calls").and_then(|c| c.as_array())
                        {
                            for tool_call in tool_calls {
                                if let (Some(tool_id), Some(function)) = (
                                    tool_call.get("id").and_then(|v| v.as_str()),
                                    tool_call.get("function"),
                                ) && let Some(tool_name) =
                                    function.get("name").and_then(|v| v.as_str())
                                {
                                    tool_name_by_id
                                        .insert(tool_id.to_string(), tool_name.to_string());
                                }
                            }
                        }
                        // Track kimi tool results: tool with tool_call_id
                        if let Some(role) = json.get("role").and_then(|r| r.as_str())
                            && role == "tool"
                            && let Some(tool_call_id) =
                                json.get("tool_call_id").and_then(|v| v.as_str())
                            && let Some(tool_name) = tool_name_by_id.remove(tool_call_id)
                            && let Some(obj) = json.as_object_mut()
                        {
                            obj.insert("tool_name".to_string(), JsonValue::String(tool_name));
                        }
                        if let Some(id) = extract_session_id_from_json(&json)
                            && let Ok(mut guard) = session_id_buf.lock()
                        {
                            *guard = Some(id);
                        }
                        display_filtered_json(
                            &json,
                            &sink,
                            output_handler.as_ref(),
                            output_stream,
                        )?;
                    } else if !line_buf.trim().is_empty() {
                        let mut line = line_buf.clone();
                        sink.write_all(line.as_bytes(), output_stream)?;
                        sink.write_all(b"\n", output_stream)?;
                        if let Some(handler) = &output_handler {
                            line.push('\n');
                            handler(&line);
                        }
                    }
                    line_buf.clear();
                } else if line_buf.len() >= MAX_LINE_LENGTH {
                    // Buffer limit reached - mark as exceeded but continue reading until newline
                    line_length_exceeded = true;
                } else {
                    line_buf.push(ch);
                }
            }

            let mut guard = buffer
                .lock()
                .map_err(|_| anyhow::anyhow!("lock output buffer"))?;

            append_to_buffer(&mut guard, &text, &mut buffer_exceeded_logged);
        }

        if !line_buf.trim().is_empty() {
            if line_length_exceeded {
                log::warn!(
                    "Runner output line exceeded {}MB limit; truncating",
                    MAX_LINE_LENGTH / (1024 * 1024)
                );
            }
            let mut line = line_buf.clone();
            sink.write_all(line.as_bytes(), output_stream)?;
            sink.write_all(b"\n", output_stream)?;
            if let Some(handler) = &output_handler {
                line.push('\n');
                handler(&line);
            }
        }
        Ok(())
    })
}

pub(super) fn extract_display_lines(json: &JsonValue) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(result) = json.get("result").and_then(|r| r.as_str())
        && !result.is_empty()
    {
        lines.push(result.to_string());
    }

    if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
        if event_type == "assistant"
            && let Some(message) = json.get("message")
            && let Some(content) = message.get("content").and_then(|c| c.as_array())
        {
            for item in content {
                if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                    match item_type {
                        "text" => {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                                lines.push(text.to_string());
                            }
                        }
                        "thinking" | "analysis" | "reasoning" => {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str())
                                && !text.is_empty()
                            {
                                lines.push(outpututil::format_reasoning(text));
                            }
                        }
                        "tool_use" => {
                            if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                let details = item.get("input").and_then(format_tool_details);
                                lines.push(outpututil::format_tool_call(name, details.as_deref()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if (event_type == "item.completed" || event_type == "item.started")
            && let Some(item) = json.get("item")
            && let Some(item_type) = item.get("type").and_then(|t| t.as_str())
        {
            match item_type {
                "agent_message" => {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str())
                        && !text.is_empty()
                    {
                        lines.push(text.to_string());
                        lines.push(String::new());
                    }
                }
                "reasoning" => {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str())
                        && !text.is_empty()
                    {
                        lines.push(outpututil::format_reasoning(text));
                    }
                }
                "mcp_tool_call" => {
                    if let Some(line) = format_codex_tool_line(item) {
                        lines.push(line);
                    }
                }
                "command_execution" => {
                    if let Some(line) = format_codex_command_line(item) {
                        lines.push(line);
                    }
                }
                _ => {}
            }
        }

        if event_type == "text"
            && let Some(text) = json
                .get("part")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
        {
            lines.push(text.to_string());
        }

        if event_type == "tool_use"
            && let Some(tool) = json
                .get("part")
                .and_then(|p| p.get("tool"))
                .and_then(|t| t.as_str())
        {
            let status = json
                .get("part")
                .and_then(|p| p.get("state"))
                .and_then(|s| s.get("status"))
                .and_then(|s| s.as_str());
            let status_suffix = status.map(|value| format!("({value})"));
            let details = json
                .get("part")
                .and_then(|p| {
                    p.get("state")
                        .and_then(|s| s.get("input"))
                        .or_else(|| p.get("input"))
                })
                .and_then(format_tool_details);
            let full_details = match (status_suffix.as_deref(), details.as_deref()) {
                (None, None) => None,
                (None, Some(d)) => Some(d.to_string()),
                (Some(s), None) => Some(s.to_string()),
                (Some(s), Some(d)) => Some(format!("{} {}", s, d)),
            };
            lines.push(outpututil::format_tool_call(tool, full_details.as_deref()));
        }

        if event_type == "message" {
            let role = json.get("role").and_then(|r| r.as_str());
            if role == Some("assistant")
                && let Some(content) = json.get("content")
            {
                match content {
                    JsonValue::String(text) => {
                        if !text.is_empty() {
                            lines.push(text.clone());
                        }
                    }
                    JsonValue::Array(items) => {
                        for item in items {
                            if let Some(text) = item.get("text").and_then(|t| t.as_str())
                                && !text.is_empty()
                            {
                                lines.push(text.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if event_type == "message_end"
            && let Some(message) = json.get("message")
        {
            let role = message.get("role").and_then(|r| r.as_str());
            match role {
                Some("assistant") => {
                    if let Some(content) = message.get("content") {
                        match content {
                            JsonValue::String(text) => {
                                if !text.is_empty() {
                                    lines.push(text.clone());
                                }
                            }
                            JsonValue::Array(items) => {
                                for item in items {
                                    if let Some(text) = item.get("text").and_then(|t| t.as_str())
                                        && !text.is_empty()
                                    {
                                        lines.push(text.to_string());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Some("toolResult") => {
                    let tool = message
                        .get("toolName")
                        .and_then(|t| t.as_str())
                        .unwrap_or("tool");
                    let is_error = message
                        .get("isError")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let status = if is_error { "error" } else { "completed" };
                    lines.push(outpututil::format_tool_call(
                        tool,
                        Some(&format!("({status})")),
                    ));
                }
                _ => {}
            }
        }

        if event_type == "tool_call"
            && let Some(tool_call) = json.get("tool_call")
        {
            if let Some(mcp) = tool_call.get("mcpToolCall") {
                if let Some(args) = mcp.get("args") {
                    let tool_name = args
                        .get("providerIdentifier")
                        .and_then(|v| v.as_str())
                        .and_then(|provider| {
                            args.get("toolName")
                                .and_then(|v| v.as_str())
                                .map(|name| format!("{provider}.{name}"))
                        })
                        .or_else(|| {
                            args.get("name")
                                .and_then(|v| v.as_str())
                                .map(|name| name.to_string())
                        });
                    if let Some(tool_name) = tool_name {
                        let details = args.get("args").and_then(format_tool_details);
                        lines.push(outpututil::format_tool_call(&tool_name, details.as_deref()));
                    }
                }
            } else if let Some(shell) = tool_call.get("shellToolCall")
                && let Some(args) = shell.get("args")
            {
                let details = format_tool_details(args);
                lines.push(outpututil::format_tool_call("shell", details.as_deref()));
            }
        }

        if event_type == "tool_use"
            && let Some(tool) = json.get("tool_name").and_then(|t| t.as_str())
        {
            let details = json.get("parameters").and_then(format_tool_details);
            lines.push(outpututil::format_tool_call(tool, details.as_deref()));
        }

        if event_type == "tool_result"
            && let Some(tool) = json.get("tool_name").and_then(|t| t.as_str())
        {
            let status = json
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("completed");
            lines.push(outpututil::format_tool_call(
                tool,
                Some(&format!("({status})")),
            ));
        }
    }

    if let Some(denials) = json.get("permission_denials").and_then(|d| d.as_array()) {
        for denial in denials {
            if let Some(tool_name) = denial.get("tool_name").and_then(|t| t.as_str()) {
                lines.push(outpututil::format_permission_denied(tool_name));
            }
        }
    }

    // Handle kimi format: top-level role="assistant" with content array
    // Kimi uses this format instead of type="message" wrapper
    if let Some(role) = json.get("role").and_then(|r| r.as_str())
        && role == "assistant"
        && let Some(content) = json.get("content").and_then(|c| c.as_array())
    {
        for item in content {
            if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                match item_type {
                    "text" => {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str())
                            && !text.is_empty()
                        {
                            lines.push(text.to_string());
                        }
                    }
                    "think" => {
                        if let Some(think) = item.get("think").and_then(|t| t.as_str())
                            && !think.is_empty()
                        {
                            lines.push(outpututil::format_reasoning(think));
                        }
                    }
                    _ => {}
                }
            }
        }
        // Display kimi tool calls
        if let Some(tool_calls) = json.get("tool_calls").and_then(|c| c.as_array()) {
            for tool_call in tool_calls {
                if let Some(function) = tool_call.get("function")
                    && let Some(name) = function.get("name").and_then(|n| n.as_str())
                {
                    let details = function
                        .get("arguments")
                        .and_then(|a| a.as_str())
                        .and_then(|args_str| {
                            serde_json::from_str::<JsonValue>(args_str)
                                .inspect_err(|e| {
                                    log::trace!("Failed to parse tool arguments JSON: {}", e)
                                })
                                .ok()
                        })
                        .and_then(|args_json| format_tool_details(&args_json));
                    lines.push(outpututil::format_tool_call(name, details.as_deref()));
                }
            }
        }
    }

    // Handle kimi tool results: role="tool" with content array
    // Note: We intentionally don't display the full tool output content here
    // because it's often verbose (file contents, search results, etc.).
    // The tool arguments were already displayed when the assistant made the tool call.
    if let Some(role) = json.get("role").and_then(|r| r.as_str())
        && role == "tool"
    {
        // Get tool name (with "Tool" as default)
        let tool_name = json
            .get("tool_name")
            .and_then(|t| t.as_str())
            .unwrap_or("Tool");

        lines.push(outpututil::format_tool_call(tool_name, Some("(completed)")));
    }

    lines
}

fn format_codex_tool_line(item: &JsonValue) -> Option<String> {
    let server = item.get("server").and_then(|s| s.as_str());
    let tool = item.get("tool").and_then(|t| t.as_str());
    let name = match (server, tool) {
        (Some(server), Some(tool)) => format!("{}.{}", server, tool),
        (Some(server), None) => server.to_string(),
        (None, Some(tool)) => tool.to_string(),
        (None, None) => return None,
    };

    let status = item.get("status").and_then(|s| s.as_str());
    let status_part = status.map(|s| format!("({})", s));

    let details = item
        .get("arguments")
        .or_else(|| item.get("args"))
        .or_else(|| item.get("input"))
        .and_then(format_tool_details);

    let full_details = match (status_part, details) {
        (None, None) => None,
        (Some(s), None) => Some(s),
        (None, Some(d)) => Some(d),
        (Some(s), Some(d)) => Some(format!("{} {}", s, d)),
    };

    Some(outpututil::format_tool_call(&name, full_details.as_deref()))
}

fn format_codex_command_line(item: &JsonValue) -> Option<String> {
    let command = item.get("command").and_then(|c| c.as_str())?;
    let status = item.get("status").and_then(|s| s.as_str());
    let status_part = match (status, item.get("exit_code").and_then(|code| code.as_i64())) {
        (Some(s), Some(exit)) => Some(format!("{} (exit {})", s, exit)),
        (Some(s), None) => Some(s.to_string()),
        (None, Some(exit)) => Some(format!("exit {}", exit)),
        (None, None) => None,
    };
    Some(outpututil::format_command(command, status_part.as_deref()))
}

fn format_tool_details(input: &JsonValue) -> Option<String> {
    let object = input.as_object()?;
    let mut parts = Vec::new();

    if let Some(action) = lookup_string(object, &["action", "op", "fn"]) {
        parts.push(format!("action={}", action));
    }

    if let Some(path) = lookup_string(object, &["path", "file_path", "filePath"]) {
        parts.push(format!("path={}", path));
    }

    if let Some(paths) = lookup_array_len(object, &["paths", "file_paths", "files"]) {
        parts.push(format!("paths={}", paths));
    }

    if let Some(command) = lookup_string(object, &["command", "cmd"]) {
        let value = normalize_tool_value(&command);
        parts.push(format!("cmd={}", truncate_tool_value(&value)));
    }

    if let Some(pattern) = lookup_string(object, &["pattern", "glob", "query"]) {
        let value = normalize_tool_value(&pattern);
        parts.push(format!("pattern={}", truncate_tool_value(&value)));
    }

    if let Some(content) = lookup_string(object, &["content", "text", "message"]) {
        let value = normalize_tool_value(&content);
        parts.push(format!("content_len={}", content.len()));
        if !value.is_empty() {
            parts.push(format!("content={}", truncate_tool_value(&value)));
        }
    }

    if let Some(edits) = lookup_array_len(object, &["edits", "slices"]) {
        parts.push(format!("edits={}", edits));
    }

    // Task/subagent tool fields
    if let Some(description) = lookup_string(object, &["description"]) {
        parts.push(format!("desc={}", truncate_tool_value(&description)));
    }

    if let Some(subagent_name) = lookup_string(object, &["subagent_name"]) {
        parts.push(format!("agent={}", subagent_name));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn lookup_string(object: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if let Some(text) = value.as_str() {
                return Some(text.to_string());
            }
            if value.is_number() || value.is_boolean() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn lookup_array_len(object: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<usize> {
    for key in keys {
        if let Some(value) = object.get(*key)
            && let Some(array) = value.as_array()
        {
            return Some(array.len());
        }
    }
    None
}

fn normalize_tool_value(value: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in value.trim().chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            last_space = false;
            out.push(ch);
        }
    }
    out
}

fn truncate_tool_value(value: &str) -> String {
    if value.len() <= TOOL_VALUE_MAX_LEN {
        return value.to_string();
    }
    let mut out = String::new();
    for ch in value.chars().take(TOOL_VALUE_MAX_LEN - 1) {
        out.push(ch);
    }
    out.push('…');
    out
}

/// Display meaningful content from JSON, filtering noise.
pub(super) fn display_filtered_json(
    json: &JsonValue,
    sink: &StreamSink,
    output_handler: Option<&OutputHandler>,
    output_stream: OutputStream,
) -> anyhow::Result<()> {
    for mut line in extract_display_lines(json) {
        sink.write_all(line.as_bytes(), output_stream)?;
        sink.write_all(b"\n", output_stream)?;
        if let Some(handler) = output_handler {
            line.push('\n');
            handler(&line);
        }
    }

    Ok(())
}
