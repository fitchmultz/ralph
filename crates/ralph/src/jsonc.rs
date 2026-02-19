//! JSONC parsing utilities for Ralph.
//!
//! Responsibilities:
//! - Provide JSONC (JSON with Comments) parsing with comment support.
//! - Maintain backward compatibility with standard JSON.
//! - Integrate with existing JSON repair logic for malformed files.
//!
//! Not handled here:
//! - File I/O (callers read/write file contents).
//! - Round-tripping comments (comments are stripped on rewrite).
//!
//! Invariants/assumptions:
//! - Input is valid UTF-8.
//! - jsonc-parser is used for parsing; serde_json for serialization.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

/// Parse JSONC (JSON with Comments) into a typed struct.
/// Falls back to standard JSON parsing for backward compatibility.
pub fn parse_jsonc<T: DeserializeOwned>(raw: &str, context: &str) -> Result<T> {
    // Try JSONC parsing first (handles comments and trailing commas)
    match jsonc_parser::parse_to_serde_value(raw, &Default::default()) {
        Ok(Some(value)) => {
            serde_json::from_value(value).with_context(|| format!("parse {} from JSONC", context))
        }
        Ok(None) => {
            // Empty file case - try parsing as empty JSON (will likely fail, but gives proper error)
            serde_json::from_str::<T>(raw)
                .with_context(|| format!("parse {} as JSON (empty file)", context))
        }
        Err(jsonc_err) => {
            // Fall back to standard JSON for backward compatibility
            serde_json::from_str::<T>(raw)
                .with_context(|| format!("parse {} as JSON/JSONC: {}", context, jsonc_err))
        }
    }
}

/// Serialize to pretty-printed JSON (no comments preserved).
/// Output is always standard JSON format.
pub fn to_string_pretty<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value).context("serialize to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde::Serialize;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct TestConfig {
        version: u32,
        name: String,
    }

    #[test]
    fn parse_jsonc_accepts_standard_json() {
        let json = r#"{"version": 1, "name": "test"}"#;
        let config: TestConfig = parse_jsonc(json, "test config").unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.name, "test");
    }

    #[test]
    fn parse_jsonc_accepts_single_line_comments() {
        let jsonc = r#"{
            // This is a comment
            "version": 1,
            "name": "test"
        }"#;
        let config: TestConfig = parse_jsonc(jsonc, "test config").unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.name, "test");
    }

    #[test]
    fn parse_jsonc_accepts_multi_line_comments() {
        let jsonc = r#"{
            /* This is a
               multi-line comment */
            "version": 1,
            "name": "test"
        }"#;
        let config: TestConfig = parse_jsonc(jsonc, "test config").unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.name, "test");
    }

    #[test]
    fn parse_jsonc_accepts_trailing_commas() {
        let jsonc = r#"{
            "version": 1,
            "name": "test",
        }"#;
        let config: TestConfig = parse_jsonc(jsonc, "test config").unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.name, "test");
    }

    #[test]
    fn parse_jsonc_rejects_invalid_json() {
        let invalid = r#"{"version": 1, "name": }"#;
        let result: Result<TestConfig> = parse_jsonc(invalid, "test config");
        assert!(result.is_err());
    }

    #[test]
    fn to_string_pretty_outputs_valid_json() {
        let config = TestConfig {
            version: 1,
            name: "test".to_string(),
        };
        let json = to_string_pretty(&config).unwrap();
        // Verify it's valid JSON by parsing it back
        let _: TestConfig = serde_json::from_str(&json).unwrap();
        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("\"name\": \"test\""));
    }

    #[test]
    fn parse_jsonc_handles_mixed_comments_and_trailing_commas() {
        use serde::Deserialize;

        #[derive(Debug, Deserialize, PartialEq)]
        struct Task {
            id: String,
            title: String,
        }

        #[derive(Debug, Deserialize, PartialEq)]
        struct QueueFile {
            version: u32,
            tasks: Vec<Task>,
        }

        let jsonc = r#"{
            // Single line comment
            "version": 1,
            /* Multi-line
               comment */
            "tasks": [{
                "id": "RQ-0001",
                "title": "Test", // inline comment
            },]
        }"#;

        let result: Result<QueueFile> = parse_jsonc(jsonc, "test queue");
        assert!(
            result.is_ok(),
            "Should parse JSONC with mixed comments and trailing commas: {:?}",
            result.err()
        );
        let queue = result.unwrap();
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
    }
}
