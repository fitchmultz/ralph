//! Shared tool-call detail formatting for stream event display.
//!
//! Purpose:
//! - Shared tool-call detail formatting for stream event display.
//!
//! Responsibilities:
//! - Format compact tool-call and command lines across runner event families.
//! - Normalize and truncate detail payloads for terminal display.
//!
//! Non-scope:
//! - Stream reading or JSON event traversal.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::constants::buffers::TOOL_VALUE_MAX_LEN;
use crate::outpututil;
use serde_json::Value as JsonValue;

pub(super) fn format_codex_tool_line(item: &JsonValue) -> Option<String> {
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

pub(super) fn format_codex_command_line(item: &JsonValue) -> Option<String> {
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

pub(super) fn format_tool_details(input: &JsonValue) -> Option<String> {
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
