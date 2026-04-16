//! Cursor-style tool call extraction.
//!
//! Responsibilities:
//! - Render nested `tool_call` envelopes used by Cursor-style runner output.
//! - Preserve tool argument summaries through shared detail formatting.
//!
//! Does not handle:
//! - Codex command items.
//! - Gemini/Kimi role-based assistant payloads.

use crate::outpututil;
use serde_json::Value as JsonValue;

use super::super::stream_tool_details::format_tool_details;

pub(super) fn collect_lines(json: &JsonValue, lines: &mut Vec<String>) {
    if json.get("type").and_then(|t| t.as_str()) != Some("tool_call") {
        return;
    }

    let Some(tool_call) = json.get("tool_call") else {
        return;
    };

    let subtype = json.get("subtype").and_then(|s| s.as_str());
    if let Some(line) = format_cursor_tool_call(tool_call, subtype) {
        lines.push(line);
        return;
    }

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

fn format_cursor_tool_call(tool_call: &JsonValue, subtype: Option<&str>) -> Option<String> {
    if let Some(read) = tool_call.get("readToolCall")
        && let Some(args) = read.get("args")
    {
        return Some(format_named_tool_call("read_file", args, subtype));
    }

    if let Some(write) = tool_call.get("writeToolCall")
        && let Some(args) = write.get("args")
    {
        return Some(format_named_tool_call("write_file", args, subtype));
    }

    if let Some(func) = tool_call.get("function")
        && let Some(name) = func.get("name").and_then(|n| n.as_str())
    {
        let args = func.get("arguments").or_else(|| func.get("args"));
        let details = args.and_then(format_tool_details);
        let label = match subtype {
            Some("started") => name.to_string(),
            Some("completed") => format!("{name} (completed)"),
            Some(other) => format!("{name} ({other})"),
            None => name.to_string(),
        };
        return Some(outpututil::format_tool_call(&label, details.as_deref()));
    }

    None
}

fn format_named_tool_call(name: &str, args: &JsonValue, subtype: Option<&str>) -> String {
    let details = format_tool_details(args);
    let label = match subtype {
        Some("started") => name.to_string(),
        Some("completed") => format!("{name} (completed)"),
        Some(other) => format!("{name} ({other})"),
        None => name.to_string(),
    };
    outpututil::format_tool_call(&label, details.as_deref())
}

#[cfg(test)]
mod tests {
    use super::super::extract_display_lines;
    use serde_json::json;

    #[test]
    fn cursor_read_tool_call_started_renders_path() {
        let event = json!({
            "type": "tool_call",
            "subtype": "started",
            "call_id": "call_1",
            "tool_call": {
                "readToolCall": {
                    "args": { "path": "README.md" }
                }
            },
            "session_id": "sess"
        });

        let lines = extract_display_lines(&event);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("read_file") && l.contains("README.md")),
            "{lines:?}"
        );
    }

    #[test]
    fn cursor_write_tool_call_completed_renders_without_dumping_file_contents() {
        let event = json!({
            "type": "tool_call",
            "subtype": "completed",
            "call_id": "call_2",
            "tool_call": {
                "writeToolCall": {
                    "args": {
                        "path": "summary.txt",
                        "fileText": "pretend this is huge",
                        "toolCallId": "call_2"
                    }
                }
            },
            "session_id": "sess"
        });

        let lines = extract_display_lines(&event);
        let joined = lines.join(" ");
        assert!(joined.contains("write_file"));
        assert!(joined.contains("summary.txt"));
        assert!(!joined.contains("pretend this is huge"));
    }
}
