//! Claude-style stream event extraction.
//!
//! Purpose:
//! - Claude-style stream event extraction.
//!
//! Responsibilities:
//! - Render Claude assistant message content, tool use, and terminal error payloads.
//! - Handle `message_end` tool result formatting.
//!
//! Non-scope:
//! - Codex item streams.
//! - OpenCode/Cursor/Gemini/Kimi-specific payloads.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::outpututil;
use serde_json::Value as JsonValue;

use super::super::stream_tool_details::format_tool_details;
use super::common;

pub(super) fn collect_lines(json: &JsonValue, lines: &mut Vec<String>) {
    let Some(event_type) = json.get("type").and_then(|t| t.as_str()) else {
        return;
    };

    if event_type == "assistant"
        && let Some(message) = json.get("message")
        && let Some(content) = message.get("content").and_then(|c| c.as_array())
    {
        for item in content {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("text") => common::push_text(lines, item.get("text").and_then(|t| t.as_str())),
                Some("thinking" | "analysis" | "reasoning") => {
                    common::push_reasoning(lines, item.get("text").and_then(|t| t.as_str()))
                }
                Some("tool_use") => {
                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                        let details = item.get("input").and_then(format_tool_details);
                        lines.push(outpututil::format_tool_call(name, details.as_deref()));
                    }
                }
                _ => {}
            }
        }
    }

    if event_type == "message_end"
        && let Some(message) = json.get("message")
    {
        match message.get("role").and_then(|r| r.as_str()) {
            Some("assistant") => push_message_content(message.get("content"), lines),
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

    if event_type == "result"
        && json
            .get("is_error")
            .and_then(|flag| flag.as_bool())
            .unwrap_or(false)
    {
        if let Some(errors) = json.get("errors").and_then(|e| e.as_array()) {
            for error in errors {
                common::push_error(lines, error.as_str());
            }
        } else {
            common::push_error(
                lines,
                json.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str()),
            );
        }
    }
}

fn push_message_content(content: Option<&JsonValue>, lines: &mut Vec<String>) {
    match content {
        Some(JsonValue::String(text)) => common::push_text(lines, Some(text)),
        Some(JsonValue::Array(items)) => {
            for item in items {
                common::push_text(lines, item.get("text").and_then(|t| t.as_str()));
            }
        }
        _ => {}
    }
}
