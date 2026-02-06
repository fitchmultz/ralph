//! Response extraction helpers for runner output streams.
//!
//! Responsibilities: parse streaming runner JSON output and extract final assistant responses.
//! Not handled: executing runners, managing processes, or validating runner configurations.
//! Invariants/assumptions: stdout lines are JSON fragments emitted by supported runners.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::contracts::Runner;

use super::builtin_plugins::{CodexResponseParser, KimiResponseParser};
use super::json::parse_json_line;
use super::plugin_trait::ResponseParser;

/// Registry of response parsers by runner.
pub struct ResponseParserRegistry {
    parsers: HashMap<String, Box<dyn ResponseParser>>,
}

impl Default for ResponseParserRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseParserRegistry {
    /// Create a new registry with all built-in parsers registered.
    pub fn new() -> Self {
        let mut parsers: HashMap<String, Box<dyn ResponseParser>> = HashMap::new();

        // Register all built-in parsers
        parsers.insert("codex".to_string(), Box::new(CodexResponseParser));
        parsers.insert("claude".to_string(), Box::new(ClaudeResponseParser));
        parsers.insert("kimi".to_string(), Box::new(KimiResponseParser));
        parsers.insert("gemini".to_string(), Box::new(GeminiResponseParser));
        parsers.insert("opencode".to_string(), Box::new(OpencodeResponseParser));
        parsers.insert("pi".to_string(), Box::new(PiResponseParser));
        parsers.insert("cursor".to_string(), Box::new(CursorResponseParser));

        Self { parsers }
    }

    /// Register a custom parser for a runner.
    #[allow(dead_code)]
    pub fn register(&mut self, runner_id: String, parser: Box<dyn ResponseParser>) {
        self.parsers.insert(runner_id, parser);
    }

    /// Extract the final assistant response from runner output.
    pub fn extract_final_response(&self, runner: &Runner, stdout: &str) -> Option<String> {
        let runner_id = runner.id();
        let parser = self.parsers.get(runner_id)?;

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

            if let Some(text) = parser.parse(&json, &mut streaming_buffer) {
                final_message = Some(text);
                streaming_buffer.clear();
            }
        }

        final_message
    }
}

// =============================================================================
// Runner-Specific Response Parsers
// =============================================================================

struct ClaudeResponseParser;

impl ResponseParser for ClaudeResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            return None;
        }

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

    fn runner_id(&self) -> &str {
        "claude"
    }
}

struct GeminiResponseParser;

impl ResponseParser for GeminiResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("message") {
            return None;
        }

        if json.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            return None;
        }

        let content = json.get("content")?;
        extract_text_content(content)
    }

    fn runner_id(&self) -> &str {
        "gemini"
    }
}

struct OpencodeResponseParser;

impl ResponseParser for OpencodeResponseParser {
    fn parse(&self, json: &JsonValue, buffer: &mut String) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("text") {
            return None;
        }

        let text = json
            .get("part")
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())?;

        if text.is_empty() {
            return None;
        }

        // Opencode streams text incrementally
        buffer.push_str(text);
        Some(buffer.clone())
    }

    fn runner_id(&self) -> &str {
        "opencode"
    }
}

struct PiResponseParser;

impl ResponseParser for PiResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("result") {
            return None;
        }

        let result = json.get("result")?;
        extract_text_content(result)
    }

    fn runner_id(&self) -> &str {
        "pi"
    }
}

struct CursorResponseParser;

impl ResponseParser for CursorResponseParser {
    fn parse(&self, json: &JsonValue, _buffer: &mut String) -> Option<String> {
        if json.get("type").and_then(|t| t.as_str()) != Some("message_end") {
            return None;
        }

        let message = json.get("message")?;
        if message.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            return None;
        }

        let content = message.get("content")?;
        extract_text_content(content)
    }

    fn runner_id(&self) -> &str {
        "cursor"
    }
}

// =============================================================================
// Legacy Compatibility
// =============================================================================

/// Extract the final assistant response from stdout using the parser registry.
///
/// This is the legacy function that maintains backward compatibility.
/// New code should use ResponseParserRegistry directly.
pub(crate) fn extract_final_assistant_response(stdout: &str) -> Option<String> {
    let registry = ResponseParserRegistry::new();

    // Try each parser until we find a match
    // This maintains backward compatibility with the old behavior
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

        // Try all parsers in order
        if let Some(text) = try_all_parsers(&json, &mut streaming_buffer, &registry) {
            final_message = Some(text);
            streaming_buffer.clear();
        }
    }

    final_message
}

/// Try all registered parsers on a JSON value.
fn try_all_parsers(
    json: &JsonValue,
    buffer: &mut String,
    _registry: &ResponseParserRegistry,
) -> Option<String> {
    // Try each built-in parser directly
    // This is more efficient than going through the registry for the legacy path

    // Codex: item.completed with agent_message
    if let Some(text) = CodexResponseParser.parse(json, buffer) {
        return Some(text);
    }

    // Claude: assistant type with message.content
    if let Some(text) = ClaudeResponseParser.parse(json, buffer) {
        return Some(text);
    }

    // Gemini: message type with role=assistant
    if let Some(text) = GeminiResponseParser.parse(json, buffer) {
        return Some(text);
    }

    // Kimi: top-level role=assistant
    if let Some(text) = KimiResponseParser.parse(json, buffer) {
        return Some(text);
    }

    // Opencode: text type with streaming
    if let Some(text) = OpencodeResponseParser.parse(json, buffer) {
        return Some(text);
    }

    // Pi: result type
    if let Some(text) = PiResponseParser.parse(json, buffer) {
        return Some(text);
    }

    // Cursor: message_end type
    if let Some(text) = CursorResponseParser.parse(json, buffer) {
        return Some(text);
    }

    None
}

// =============================================================================
// Shared Helpers
// =============================================================================

/// Extract text content from a JSON value (string or array of text objects).
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_registry_extracts_codex_response() {
        let registry = ResponseParserRegistry::new();
        let runner = Runner::Codex;

        let stdout = r#"{"type":"item.completed","item":{"type":"agent_message","text":"Hello from Codex"}}"#;

        let result = registry.extract_final_response(&runner, stdout);
        assert_eq!(result, Some("Hello from Codex".to_string()));
    }

    #[test]
    fn response_registry_extracts_kimi_response() {
        let registry = ResponseParserRegistry::new();
        let runner = Runner::Kimi;

        let stdout = r#"{"role":"assistant","content":[{"type":"text","text":"Hello from Kimi"}]}"#;

        let result = registry.extract_final_response(&runner, stdout);
        assert_eq!(result, Some("Hello from Kimi".to_string()));
    }

    #[test]
    fn response_registry_extracts_claude_response() {
        let registry = ResponseParserRegistry::new();
        let runner = Runner::Claude;

        let stdout = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello from Claude"}]}}"#;

        let result = registry.extract_final_response(&runner, stdout);
        assert_eq!(result, Some("Hello from Claude".to_string()));
    }

    #[test]
    fn legacy_extract_final_response_works() {
        let stdout =
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Legacy response"}}"#;

        let result = extract_final_assistant_response(stdout);
        assert_eq!(result, Some("Legacy response".to_string()));
    }

    #[test]
    fn extract_text_content_handles_string() {
        let json = JsonValue::String("  hello world  ".to_string());
        assert_eq!(extract_text_content(&json), Some("hello world".to_string()));
    }

    #[test]
    fn extract_text_content_handles_array() {
        let json = JsonValue::Array(vec![
            serde_json::json!({"type": "text", "text": "line 1"}),
            serde_json::json!({"type": "text", "text": "line 2"}),
        ]);
        assert_eq!(
            extract_text_content(&json),
            Some("line 1\nline 2".to_string())
        );
    }

    #[test]
    fn opencode_response_parser_accumulates_streaming_text() {
        let parser = OpencodeResponseParser;
        let mut buffer = String::new();

        let line1 = r#"{"type":"text","part":{"text":"Hello "}}"#;
        let line2 = r#"{"type":"text","part":{"text":"World"}}"#;

        let result1 = parser.parse(&serde_json::from_str(line1).unwrap(), &mut buffer);
        assert_eq!(result1, Some("Hello ".to_string()));

        let result2 = parser.parse(&serde_json::from_str(line2).unwrap(), &mut buffer);
        assert_eq!(result2, Some("Hello World".to_string()));
    }
}
