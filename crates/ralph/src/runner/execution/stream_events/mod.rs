//! JSON event normalization and display-line extraction for runner streams.
//!
//! Responsibilities:
//! - Correlate tool-use and tool-result events across runner formats.
//! - Route JSON payloads through focused protocol formatters.
//! - Preserve compact terminal-friendly display lines for stream rendering.
//!
//! Does not handle:
//! - Reading bytes from subprocess streams.
//! - Writing output to sinks or handlers.
//!
//! Assumptions/invariants:
//! - JSON values are best-effort and may be partially populated.

mod claude;
mod codex;
mod common;
mod cursor;
mod gemini;
mod kimi;
mod opencode;
mod pi;

use serde_json::Value as JsonValue;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct ToolCallTracker {
    tool_name_by_id: HashMap<String, String>,
}

impl ToolCallTracker {
    pub(super) fn correlate(&mut self, json: &mut JsonValue) {
        if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
            if event_type == "tool_use" {
                if let (Some(tool_id), Some(tool_name)) = (
                    json.get("tool_id").and_then(|v| v.as_str()),
                    json.get("tool_name").and_then(|v| v.as_str()),
                ) {
                    self.tool_name_by_id
                        .insert(tool_id.to_string(), tool_name.to_string());
                }
            } else if event_type == "tool_result" {
                let tool_id = json.get("tool_id").and_then(|v| v.as_str());
                if let Some(tool_id) = tool_id
                    && let Some(tool_name) = self.tool_name_by_id.remove(tool_id)
                    && let Some(obj) = json.as_object_mut()
                {
                    obj.insert("tool_name".to_string(), JsonValue::String(tool_name));
                }
            }
        }

        if let Some(role) = json.get("role").and_then(|r| r.as_str())
            && role == "assistant"
            && let Some(tool_calls) = json.get("tool_calls").and_then(|c| c.as_array())
        {
            for tool_call in tool_calls {
                if let (Some(tool_id), Some(function)) = (
                    tool_call.get("id").and_then(|v| v.as_str()),
                    tool_call.get("function"),
                ) && let Some(tool_name) = function.get("name").and_then(|v| v.as_str())
                {
                    self.tool_name_by_id
                        .insert(tool_id.to_string(), tool_name.to_string());
                }
            }
        }

        if let Some(role) = json.get("role").and_then(|r| r.as_str())
            && role == "tool"
            && let Some(tool_call_id) = json.get("tool_call_id").and_then(|v| v.as_str())
            && let Some(tool_name) = self.tool_name_by_id.remove(tool_call_id)
            && let Some(obj) = json.as_object_mut()
        {
            obj.insert("tool_name".to_string(), JsonValue::String(tool_name));
        }
    }
}

pub(crate) fn extract_display_lines(json: &JsonValue) -> Vec<String> {
    let mut lines = Vec::new();
    common::push_result_line(json, &mut lines);
    match classify_protocol(json) {
        StreamProtocol::Claude => claude::collect_lines(json, &mut lines),
        StreamProtocol::Codex => codex::collect_lines(json, &mut lines),
        StreamProtocol::Cursor => cursor::collect_lines(json, &mut lines),
        StreamProtocol::Gemini => gemini::collect_lines(json, &mut lines),
        StreamProtocol::Kimi => kimi::collect_lines(json, &mut lines),
        StreamProtocol::Opencode => opencode::collect_lines(json, &mut lines),
        StreamProtocol::Pi => pi::collect_lines(json, &mut lines),
        StreamProtocol::Unknown => {}
    }
    common::push_permission_denials(json, &mut lines);
    lines
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamProtocol {
    Claude,
    Codex,
    Cursor,
    Gemini,
    Kimi,
    Opencode,
    Pi,
    Unknown,
}

fn classify_protocol(json: &JsonValue) -> StreamProtocol {
    let event_type = json.get("type").and_then(|value| value.as_str());
    let role = json.get("role").and_then(|value| value.as_str());

    match event_type {
        Some("item.completed" | "item.started") => StreamProtocol::Codex,
        Some("tool_call") => StreamProtocol::Cursor,
        Some("message_update" | "tool_execution_start") => StreamProtocol::Pi,
        Some("message_end") if is_pi_message_end(json) => StreamProtocol::Pi,
        Some("assistant" | "message_end" | "result") => StreamProtocol::Claude,
        Some("message") if role == Some("assistant") => StreamProtocol::Gemini,
        Some("tool_result") => StreamProtocol::Gemini,
        Some("tool_use") => {
            if json.get("part").is_some() {
                StreamProtocol::Opencode
            } else {
                StreamProtocol::Gemini
            }
        }
        Some("text" | "reasoning" | "error") => StreamProtocol::Opencode,
        _ if role == Some("assistant") || role == Some("tool") => StreamProtocol::Kimi,
        _ => StreamProtocol::Unknown,
    }
}

fn is_pi_message_end(json: &JsonValue) -> bool {
    let Some(message) = json.get("message") else {
        return false;
    };

    if message.get("role").and_then(|role| role.as_str()) == Some("toolResult") {
        return true;
    }

    message.get("api").is_some()
        || message.get("provider").is_some()
        || message.get("responseId").is_some()
}
