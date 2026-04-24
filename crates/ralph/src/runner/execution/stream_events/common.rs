//! Shared helpers for stream event display extraction.
//!
//! Purpose:
//! - Shared helpers for stream event display extraction.
//!
//! Responsibilities:
//! - Provide small formatting helpers reused by protocol-specific collectors.
//! - Keep repeated text/error/result extraction logic out of the hot-path router.
//!
//! Non-scope:
//! - Protocol-specific event branching.
//! - Tool-call correlation.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::outpututil;
use serde_json::Value as JsonValue;

pub(super) fn push_result_line(json: &JsonValue, lines: &mut Vec<String>) {
    if let Some(result) = json.get("result").and_then(|r| r.as_str())
        && !result.is_empty()
    {
        lines.push(result.to_string());
    }
}

pub(super) fn push_text(lines: &mut Vec<String>, text: Option<&str>) {
    if let Some(text) = text
        && !text.is_empty()
    {
        lines.push(text.to_string());
    }
}

pub(super) fn push_reasoning(lines: &mut Vec<String>, text: Option<&str>) {
    if let Some(text) = text
        && !text.is_empty()
    {
        lines.push(outpututil::format_reasoning(text));
    }
}

pub(super) fn push_error(lines: &mut Vec<String>, text: Option<&str>) {
    if let Some(text) = text
        && !text.trim().is_empty()
    {
        lines.push(format!("[Error] {}", text));
    }
}

pub(super) fn push_permission_denials(json: &JsonValue, lines: &mut Vec<String>) {
    if let Some(denials) = json.get("permission_denials").and_then(|d| d.as_array()) {
        for denial in denials {
            if let Some(tool_name) = denial.get("tool_name").and_then(|t| t.as_str()) {
                lines.push(outpututil::format_permission_denied(tool_name));
            }
        }
    }
}
