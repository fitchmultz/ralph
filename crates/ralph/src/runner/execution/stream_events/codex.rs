//! Codex stream event extraction.
//!
//! Purpose:
//! - Codex stream event extraction.
//!
//! Responsibilities:
//! - Render `item.started` / `item.completed` payloads for Codex JSON streams.
//! - Format tool, command, web-search, and collab-tool lines.
//!
//! Non-scope:
//! - Claude or Gemini message formats.
//! - Kimi role-based content arrays.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::outpututil;
use serde_json::Value as JsonValue;

use super::super::stream_tool_details::{format_codex_command_line, format_codex_tool_line};
use super::common;

pub(super) fn collect_lines(json: &JsonValue, lines: &mut Vec<String>) {
    let Some(event_type) = json.get("type").and_then(|t| t.as_str()) else {
        return;
    };

    if !(event_type == "item.completed" || event_type == "item.started") {
        return;
    }

    let Some(item) = json.get("item") else {
        return;
    };

    match item.get("type").and_then(|t| t.as_str()) {
        Some("agent_message") => {
            if let Some(text) = item.get("text").and_then(|t| t.as_str())
                && !text.is_empty()
            {
                lines.push(text.to_string());
                lines.push(String::new());
            }
        }
        Some("reasoning") => {
            common::push_reasoning(lines, item.get("text").and_then(|t| t.as_str()))
        }
        Some("mcp_tool_call") => {
            if let Some(line) = format_codex_tool_line(item) {
                lines.push(line);
            }
        }
        Some("command_execution") => {
            if let Some(line) = format_codex_command_line(item) {
                lines.push(line);
            }
        }
        Some("web_search") => {
            let query = item.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let action = item.get("action").and_then(|a| a.as_str());
            let details = if query.is_empty() {
                action.map(|a| format!("action={}", a))
            } else {
                Some(match action {
                    Some(a) => format!("query={} action={}", query, a),
                    None => format!("query={}", query),
                })
            };
            lines.push(outpututil::format_tool_call(
                "web_search",
                details.as_deref(),
            ));
        }
        Some("collab_tool_call") => {
            if let Some(tool) = item.get("tool").and_then(|t| t.as_str()) {
                let details = item
                    .get("status")
                    .and_then(|s| s.as_str())
                    .map(|status| format!("({status})"));
                lines.push(outpututil::format_tool_call(
                    &format!("collab.{}", tool),
                    details.as_deref(),
                ));
            }
        }
        Some("error") => common::push_error(lines, item.get("message").and_then(|m| m.as_str())),
        _ => {}
    }
}
