//! Pi stream event extraction.
//!
//! Purpose:
//! - Normalize Pi Coding Agent stream-json envelopes into concise terminal lines.
//!
//! Responsibilities:
//! - Render completed reasoning blocks from `message_update` events.
//! - Render tool execution start lines with concrete arguments like bash commands and file paths.
//! - Render assistant text and tool completion lines from `message_end` events.
//!
//! Scope:
//! - Pi-specific NDJSON envelopes such as `message_update`, `tool_execution_start`, and
//!   Pi-flavored `message_end` payloads.
//!
//! Usage:
//! - Called by the shared stream event router after protocol classification.
//!
//! Invariants/assumptions:
//! - Pi `tool_execution_start` events expose tool arguments under `args`.
//! - Pi `thinking_end` events expose finalized reasoning text under `assistantMessageEvent.content`.

use crate::outpututil;
use serde_json::Value as JsonValue;

use super::super::stream_tool_details::format_tool_details;
use super::common;

pub(super) fn collect_lines(json: &JsonValue, lines: &mut Vec<String>) {
    match json.get("type").and_then(|t| t.as_str()) {
        Some("message_update") => collect_message_update(json, lines),
        Some("tool_execution_start") => collect_tool_execution_start(json, lines),
        Some("message_end") => collect_message_end(json, lines),
        _ => {}
    }
}

fn collect_message_update(json: &JsonValue, lines: &mut Vec<String>) {
    let Some(event) = json.get("assistantMessageEvent") else {
        return;
    };

    if event.get("type").and_then(|t| t.as_str()) == Some("thinking_end") {
        common::push_reasoning(lines, event.get("content").and_then(|t| t.as_str()));
    }
}

fn collect_tool_execution_start(json: &JsonValue, lines: &mut Vec<String>) {
    let Some(tool_name) = json.get("toolName").and_then(|t| t.as_str()) else {
        return;
    };

    let details = json.get("args").and_then(format_tool_details);
    lines.push(outpututil::format_tool_call(tool_name, details.as_deref()));
}

fn collect_message_end(json: &JsonValue, lines: &mut Vec<String>) {
    let Some(message) = json.get("message") else {
        return;
    };

    match message.get("role").and_then(|r| r.as_str()) {
        Some("assistant") => push_message_text(message.get("content"), lines),
        Some("toolResult") => push_tool_result(message, lines),
        _ => {}
    }
}

fn push_message_text(content: Option<&JsonValue>, lines: &mut Vec<String>) {
    match content {
        Some(JsonValue::String(text)) => common::push_text(lines, Some(text)),
        Some(JsonValue::Array(items)) => {
            for item in items {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    common::push_text(lines, item.get("text").and_then(|t| t.as_str()));
                }
            }
        }
        _ => {}
    }
}

fn push_tool_result(message: &JsonValue, lines: &mut Vec<String>) {
    let tool = message
        .get("toolName")
        .and_then(|t| t.as_str())
        .unwrap_or("Tool");
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
