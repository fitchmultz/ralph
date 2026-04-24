//! Kimi-style stream event extraction.
//!
//! Purpose:
//! - Kimi-style stream event extraction.
//!
//! Responsibilities:
//! - Render role-based assistant content arrays with text and think entries.
//! - Render deferred tool calls carried in `tool_calls`.
//!
//! Non-scope:
//! - Claude/Codex event typing.
//! - Cursor nested tool call envelopes.
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
    if json.get("role").and_then(|r| r.as_str()) == Some("assistant")
        && let Some(content) = json.get("content").and_then(|c| c.as_array())
    {
        for item in content {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("text") => common::push_text(lines, item.get("text").and_then(|t| t.as_str())),
                Some("think") => {
                    common::push_reasoning(lines, item.get("think").and_then(|t| t.as_str()))
                }
                _ => {}
            }
        }
    }

    if json.get("role").and_then(|r| r.as_str()) == Some("assistant")
        && let Some(tool_calls) = json.get("tool_calls").and_then(|c| c.as_array())
    {
        for tool_call in tool_calls {
            if let Some(function) = tool_call.get("function")
                && let Some(name) = function.get("name").and_then(|n| n.as_str())
            {
                let details = function
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .and_then(|args| serde_json::from_str::<JsonValue>(args).ok())
                    .and_then(|args| format_tool_details(&args));
                lines.push(outpututil::format_tool_call(name, details.as_deref()));
            }
        }
    }

    if json.get("role").and_then(|r| r.as_str()) == Some("tool") {
        let tool_name = json
            .get("tool_name")
            .and_then(|t| t.as_str())
            .unwrap_or("Tool");
        lines.push(outpututil::format_tool_call(tool_name, Some("(completed)")));
    }
}
