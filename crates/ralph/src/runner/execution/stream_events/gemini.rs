//! Gemini-style stream event extraction.
//!
//! Purpose:
//! - Gemini-style stream event extraction.
//!
//! Responsibilities:
//! - Render `tool_use` / `tool_result` events keyed by `tool_name`.
//! - Render assistant `message` payloads that surface as plain strings.
//!
//! Non-scope:
//! - Claude `message_end` semantics.
//! - Kimi tool-call arrays.
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

    match event_type {
        "tool_use" => {
            if let Some(tool) = json.get("tool_name").and_then(|t| t.as_str()) {
                let details = json.get("parameters").and_then(format_tool_details);
                lines.push(outpututil::format_tool_call(tool, details.as_deref()));
            }
        }
        "tool_result" => {
            if let Some(tool) = json.get("tool_name").and_then(|t| t.as_str()) {
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
        "message" => {
            if json.get("role").and_then(|r| r.as_str()) == Some("assistant") {
                common::push_text(lines, json.get("content").and_then(|c| c.as_str()));
            }
        }
        _ => {}
    }
}
