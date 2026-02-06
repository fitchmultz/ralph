//! Response extraction helpers for runner output streams.
//!
//! Responsibilities: parse streaming runner JSON output and extract final assistant responses.
//! Not handled: executing runners, managing processes, or validating runner configurations.
//! Invariants/assumptions: stdout lines are JSON fragments emitted by supported runners.

use serde_json::Value as JsonValue;

use super::json::parse_json_line;

/// Structured content extracted from Kimi runner responses.
#[allow(dead_code)]
pub(crate) struct KimiContent {
    pub text: Option<String>,
    /// Thinking/reasoning content (available for future use)
    pub thinking: Option<String>,
}

pub(crate) fn extract_final_assistant_response(stdout: &str) -> Option<String> {
    let mut final_message: Option<String> = None;
    let mut streaming_buffer = String::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(json) = parse_json_line(line) else {
            continue;
        };

        if let Some(event_type) = json.get("type").and_then(|t| t.as_str()) {
            match event_type {
                "result" => {
                    if let Some(text) = json.get("result").and_then(|r| r.as_str()) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            final_message = Some(trimmed.to_string());
                            streaming_buffer.clear();
                        }
                    }
                }
                "item.completed" => {
                    if let Some(text) = extract_codex_agent_message(&json) {
                        final_message = Some(text);
                        streaming_buffer.clear();
                    }
                }
                "assistant" => {
                    if let Some(text) = extract_claude_assistant_text(&json) {
                        final_message = Some(text);
                        streaming_buffer.clear();
                    }
                }
                "message" => {
                    if let Some(text) = extract_gemini_assistant_text(&json) {
                        final_message = Some(text);
                        streaming_buffer.clear();
                    }
                }
                "message_end" => {
                    if let Some(text) = extract_message_end_assistant_text(&json) {
                        final_message = Some(text);
                        streaming_buffer.clear();
                    }
                }
                "text" => {
                    if let Some(text) = extract_opencode_text(&json)
                        && !text.is_empty()
                    {
                        streaming_buffer.push_str(text);
                        final_message = Some(streaming_buffer.clone());
                    }
                }
                _ => {}
            }
        } else {
            // Check for kimi format: top-level role="assistant" without type field
            if let Some(text) = extract_kimi_assistant_text(&json) {
                final_message = Some(text);
                streaming_buffer.clear();
            }
        }
    }

    final_message
}

/// Extracts both text and thinking content from Kimi responses.
pub(crate) fn extract_kimi_content(json: &JsonValue) -> Option<KimiContent> {
    // Kimi format has role="assistant" at top level with content array
    if json.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return None;
    }

    let content = json.get("content")?;
    let items = content.as_array()?;

    let mut text_parts = Vec::new();
    let mut thinking_parts = Vec::new();

    for item in items {
        match item.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        text_parts.push(trimmed.to_string());
                    }
                }
            }
            Some("think") => {
                if let Some(think) = item.get("think").and_then(|t| t.as_str()) {
                    let trimmed = think.trim();
                    if !trimmed.is_empty() {
                        thinking_parts.push(trimmed.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    Some(KimiContent {
        text: if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        },
        thinking: if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n"))
        },
    })
}

fn extract_kimi_assistant_text(json: &JsonValue) -> Option<String> {
    extract_kimi_content(json).and_then(|content| content.text)
}

fn extract_codex_agent_message(json: &JsonValue) -> Option<String> {
    let item = json.get("item")?;
    if item.get("type").and_then(|t| t.as_str()) != Some("agent_message") {
        return None;
    }
    let text = item.get("text").and_then(|t| t.as_str())?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn extract_claude_assistant_text(json: &JsonValue) -> Option<String> {
    let message = json.get("message")?;
    let content = message.get("content")?.as_array()?;
    let mut parts = Vec::new();
    for item in content {
        if item.get("type").and_then(|t| t.as_str()) != Some("text") {
            continue;
        }
        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn extract_gemini_assistant_text(json: &JsonValue) -> Option<String> {
    if json.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return None;
    }
    let content = json.get("content")?;
    extract_text_content(content)
}

fn extract_opencode_text(json: &JsonValue) -> Option<&str> {
    json.get("part")
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
}

fn extract_message_end_assistant_text(json: &JsonValue) -> Option<String> {
    let message = json.get("message")?;
    if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return None;
    }
    let content = message.get("content")?;
    extract_text_content(content)
}

fn extract_text_content(content: &JsonValue) -> Option<String> {
    match content {
        JsonValue::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        JsonValue::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}
