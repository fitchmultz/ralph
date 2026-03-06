//! JSON repair utilities for fixing common agent-induced JSON errors.
//!
//! Responsibilities:
//! - Attempt to repair malformed JSON caused by common agent mistakes.
//! - Fix single-quoted strings, unquoted object keys, trailing commas,
//!   unescaped newlines, and missing closing brackets/braces.
//!
//! Not handled here:
//! - JSONC parsing with comments (handled by `crate::jsonc`).
//! - Semantic validation of queue content.
//!
//! Invariants/assumptions:
//! - Repair functions return `None` if no changes were made.
//! - Repairs are conservative; they should not make valid JSON invalid.

use regex::Regex;
use std::sync::LazyLock;

static SINGLE_QUOTED_STRING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(^|[^a-zA-Z0-9])'([^']*?)'([^a-zA-Z0-9]|$)").unwrap());

static UNQUOTED_KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([{,]\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*:").unwrap());

static TRAILING_COMMA_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",(\s*[}\]])").unwrap());

static TRAILING_COMMA_NEWLINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r",(\s*)\n(\s*[}\]])").unwrap());

/// Attempt to repair common JSON errors induced by agents.
/// Returns Some(repaired_json) if repairs were made, None if no repairs possible.
pub fn attempt_json_repair(raw: &str) -> Option<String> {
    let mut repaired = raw.to_string();
    let original = raw.to_string();

    // Repair 1: Convert single-quoted strings to double-quoted
    // Pattern: 'value' (but not apostrophes within words like "don't")
    // We match single quotes that appear to be string delimiters
    // Match '...' where the content doesn't contain ' and is not preceded/followed by alphanumeric
    // Use ^ or non-alphanumeric before, and non-alphanumeric or $ after
    if SINGLE_QUOTED_STRING_RE.is_match(&repaired) {
        log::debug!("JSON repair: converting single-quoted strings to double-quoted");
        repaired = SINGLE_QUOTED_STRING_RE
            .replace_all(&repaired, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let content = &caps[2];
                let suffix = &caps[3];
                let escaped = content.replace('"', "\\\"");
                format!("{}\"{}\"{}", prefix, escaped, suffix)
            })
            .to_string();
    }

    // Repair 2: Add missing quotes around unquoted object keys
    // Pattern: {[ or , followed by whitespace, then identifier followed by colon
    // Matches: {key: or ,key: or { key: or , key:
    if UNQUOTED_KEY_RE.is_match(&repaired) {
        log::debug!("JSON repair: adding quotes around unquoted object keys");
        repaired = UNQUOTED_KEY_RE
            .replace_all(&repaired, "$1\"$2\":")
            .to_string();
    }

    // Repair 3: Fix unescaped newlines within string values
    // This is a common error when agents paste multi-line content
    // We need to find newlines that are inside string contexts and escape them
    repaired = repair_unescaped_newlines(&repaired);

    // Repair 4: Fix unescaped quotes within string values
    // Find quotes inside strings that aren't escaped and escape them
    repaired = repair_unescaped_quotes(&repaired);

    // Repair 5: Remove trailing commas before ] or }
    // Pattern: ,\s*] or ,\s*}
    if TRAILING_COMMA_RE.is_match(&repaired) {
        log::debug!("JSON repair: removing trailing commas");
        repaired = TRAILING_COMMA_RE.replace_all(&repaired, "$1").to_string();
    }

    // Repair 6: Remove trailing commas at end of arrays/objects (more aggressive)
    // This handles cases where there might be newlines between comma and bracket
    // Pattern: ,(\s*)\n(\s*[}\]])
    if TRAILING_COMMA_NEWLINE_RE.is_match(&repaired) {
        log::debug!("JSON repair: removing trailing commas before newlines");
        repaired = TRAILING_COMMA_NEWLINE_RE
            .replace_all(&repaired, "$1\n$2")
            .to_string();
    }

    // Repair 7: Fix missing closing bracket at end of file
    let open_brackets = repaired.matches('[').count();
    let close_brackets = repaired.matches(']').count();
    let open_braces = repaired.matches('{').count();
    let close_braces = repaired.matches('}').count();

    if open_brackets > close_brackets {
        log::debug!(
            "JSON repair: adding {} missing closing bracket(s)",
            open_brackets - close_brackets
        );
        repaired.push_str(&"]".repeat(open_brackets - close_brackets));
    }
    if open_braces > close_braces {
        log::debug!(
            "JSON repair: adding {} missing closing brace(s)",
            open_braces - close_braces
        );
        repaired.push_str(&"}".repeat(open_braces - close_braces));
    }

    if repaired != original {
        Some(repaired)
    } else {
        None
    }
}

/// Fix unescaped newlines within JSON string values.
/// Uses a simple state machine to track whether we're inside a string.
fn repair_unescaped_newlines(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let mut in_string = false;
    let mut escaped = false;

    for ch in raw.chars() {
        if escaped {
            // Previous char was backslash, this char is escaped
            result.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => {
                escaped = true;
                result.push(ch);
            }
            '"' => {
                in_string = !in_string;
                result.push(ch);
            }
            '\n' if in_string => {
                // Newline inside string - escape it
                log::trace!("JSON repair: escaping unescaped newline in string");
                result.push_str("\\n");
            }
            '\r' if in_string => {
                // Carriage return inside string - escape it
                log::trace!("JSON repair: escaping unescaped carriage return in string");
                result.push_str("\\r");
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result
}

/// Placeholder for future unescaped quote repair within JSON string values.
///
/// Currently tracks string state but does not modify quotes. Properly escaping
/// internal quotes requires look-ahead heuristics to distinguish between:
/// - Quotes that close a string (followed by structural chars like `:`, `,`, `}`, `]`)
/// - Quotes that are content and need escaping (followed by other chars)
///
/// This is a complex repair that risks over-escaping. For now, this function
/// passes through unchanged to avoid making valid JSON invalid.
fn repair_unescaped_quotes(raw: &str) -> String {
    // Future implementation: use look-ahead to determine if a quote inside
    // a string should be escaped or is closing the string.
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::QueueFile;

    #[test]
    fn attempt_json_repair_fixes_trailing_comma_in_array() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "tags": ["a", "b",]}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"tags\": [\"a\", \"b\"]"));
        assert!(!repaired.contains("\"b\","));
    }

    #[test]
    fn attempt_json_repair_fixes_trailing_comma_in_object() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Test",}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"title\": \"Test\"}"));
        assert!(!repaired.contains("\"Test\","));
    }

    #[test]
    fn attempt_json_repair_returns_none_for_valid_json() {
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Test"}]}"#;
        assert!(attempt_json_repair(input).is_none());
    }

    #[test]
    fn attempt_json_repair_fixes_multiple_trailing_commas() {
        // Test with a complete valid task structure that includes all required fields
        let input = r#"{"version": 1, "tasks": [{"id": "RQ-0001", "title": "Test", "status": "todo", "tags": ["a", "b",], "scope": ["file",],}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
    }

    // Tests for enhanced JSON repair (RQ-0362)

    #[test]
    fn attempt_json_repair_fixes_single_quoted_strings() {
        let input = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        // Check specific conversions
        assert!(repaired.contains("\"version\""));
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"id\""));
        assert!(repaired.contains("\"RQ-0001\""));
        assert!(repaired.contains("\"title\""));
        assert!(repaired.contains("\"Test\""));
    }

    #[test]
    fn attempt_json_repair_preserves_apostrophes_in_words() {
        // Apostrophes within words (like "don't") should not be converted
        let input = r#"{"tasks": [{"id": "RQ-0001", "title": "Don't break this"}]}"#;
        // This is valid JSON, so no repair needed
        assert!(attempt_json_repair(input).is_none());
    }

    #[test]
    fn attempt_json_repair_fixes_unquoted_object_keys() {
        let input = r#"{version: 1, tasks: [{id: "RQ-0001", title: "Test"}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        // Check keys are quoted
        assert!(repaired.contains("\"version\""));
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"id\""));
        assert!(repaired.contains("\"title\""));
    }

    #[test]
    fn attempt_json_repair_fixes_unquoted_keys_after_comma() {
        let input =
            r#"{"version": 1, tasks: [{"id": "RQ-0001", "title": "Test", status: "todo"}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"status\""));
    }

    #[test]
    fn attempt_json_repair_fixes_unescaped_newlines_in_strings() {
        // Agent pastes multi-line content without escaping
        let input = "{\"version\": 1, \"tasks\": [{\"id\": \"RQ-0001\", \"title\": \"Line one\nLine two\"}]}";
        let repaired = attempt_json_repair(input).expect("should repair");
        // Newlines should be escaped
        assert!(repaired.contains("Line one\\nLine two"));
        assert!(!repaired.contains("Line one\nLine two"));
        // Verify it's valid JSON
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
    }

    #[test]
    fn attempt_json_repair_fixes_unescaped_carriage_returns_in_strings() {
        let input = "{\"version\": 1, \"tasks\": [{\"id\": \"RQ-0001\", \"title\": \"Line one\rLine two\"}]}";
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("Line one\\rLine two"));
        assert!(!repaired.contains("Line one\rLine two"));
    }

    #[test]
    fn attempt_json_repair_handles_multiple_errors() {
        // Combine multiple errors: single quotes, unquoted keys, trailing comma
        let input = r#"{'version': 1, tasks: [{'id': 'RQ-0001', 'title': 'Test', 'status': 'todo', 'tags': [], 'scope': [], 'evidence': [], 'plan': [],}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        assert!(repaired.contains("\"version\""));
        assert!(repaired.contains("\"tasks\""));
        assert!(repaired.contains("\"id\""));
        assert!(repaired.contains("\"RQ-0001\""));
    }

    #[test]
    fn attempt_json_repair_escapes_double_quotes_in_single_quoted_strings() {
        // Single-quoted string containing double quotes should escape them
        let input = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Say "hello"'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains("\"Say \\\"hello\\\"\""));
    }

    #[test]
    fn attempt_json_repair_handles_empty_single_quoted_string() {
        let input = r#"{'version': 1, 'tasks': [{'id': '', 'title': ''}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should be valid JSON");
        assert!(repaired.contains("\"id\": \"\""));
    }

    #[test]
    fn attempt_json_repair_preserves_single_quote_then_unquoted_key_order() {
        let input = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'Test', 'status': 'todo', 'tags': [], 'scope': [], 'evidence': [], 'plan': [], 'created_at': '2026-01-01T00:00:00Z', 'updated_at': '2026-01-01T00:00:00Z'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        assert!(repaired.contains(r#""tasks""#));
        assert!(repaired.contains(r#""id": "RQ-0001""#));
        let _: QueueFile = serde_json::from_str(&repaired).expect("repaired should parse as JSON");
    }

    #[test]
    fn attempt_json_repair_handles_multiple_ordered_errors() {
        let input = r#"{'version': 1, tasks: [{id: 'RQ-0001', title: 'A', status: 'todo', tags: ['bug',], scope: [], evidence: [], plan: [], created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z'}]}"#;
        let repaired = attempt_json_repair(input).expect("should repair");
        let _parsed: QueueFile =
            serde_json::from_str(&repaired).expect("repaired should parse as JSON");
        assert!(repaired.contains(r#""version""#));
        assert!(repaired.contains(r#""tasks""#));
        assert!(repaired.contains(r#""title""#));
        assert!(repaired.contains(r#""tags": ["bug"]"#));
    }

    #[test]
    #[ignore = "perf-smoke: run manually when tuning hot-path: cargo test -p ralph-agent-loop queue::json_repair::tests::attempt_json_repair_perf_smoke -- --ignored"]
    fn attempt_json_repair_perf_smoke() {
        let input = r#"{'version': 1, 'tasks': [{'id': 'RQ-0001', 'title': 'A', 'status': 'todo', 'scope': ['x',], 'evidence': ['a',], 'plan': ['x',], 'created_at': '2026-01-01T00:00:00Z', 'updated_at': '2026-01-01T00:00:00Z'}]}"#;
        let start = std::time::Instant::now();
        for _ in 0..20_000 {
            let _ = attempt_json_repair(input);
        }
        let _elapsed = start.elapsed();
    }
}
