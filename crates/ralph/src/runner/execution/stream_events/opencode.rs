//! OpenCode stream event extraction.
//!
//! Purpose:
//! - OpenCode stream event extraction.
//!
//! Responsibilities:
//! - Render plain `text`, `reasoning`, `error`, and tool-use events emitted by OpenCode.
//! - Handle assistant `message` content carried outside the Claude schema.
//!
//! Non-scope:
//! - Codex item streams or Cursor tool call envelopes.
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
        "text" => common::push_text(
            lines,
            json.get("part")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str()),
        ),
        "reasoning" => common::push_reasoning(
            lines,
            json.get("part")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str()),
        ),
        "error" => common::push_error(lines, json.get("message").and_then(|m| m.as_str())),
        "tool_use" => {
            if let Some(tool) = json
                .get("part")
                .and_then(|p| p.get("tool"))
                .and_then(|t| t.as_str())
            {
                let status = json
                    .get("part")
                    .and_then(|p| p.get("state"))
                    .and_then(|s| s.get("status"))
                    .and_then(|s| s.as_str())
                    .map(|value| format!("({value})"));
                let details = json
                    .get("part")
                    .and_then(|p| {
                        p.get("state")
                            .and_then(|s| s.get("input"))
                            .or_else(|| p.get("input"))
                    })
                    .and_then(format_tool_details);
                let merged = match (status.as_deref(), details.as_deref()) {
                    (None, None) => None,
                    (None, Some(details)) => Some(details.to_string()),
                    (Some(status), None) => Some(status.to_string()),
                    (Some(status), Some(details)) => Some(format!("{} {}", status, details)),
                };
                lines.push(outpututil::format_tool_call(tool, merged.as_deref()));
            }
        }
        "message" => {
            if json.get("role").and_then(|r| r.as_str()) == Some("assistant")
                && let Some(content) = json.get("content")
            {
                match content {
                    JsonValue::String(text) => common::push_text(lines, Some(text)),
                    JsonValue::Array(items) => {
                        for item in items {
                            common::push_text(lines, item.get("text").and_then(|t| t.as_str()));
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}
