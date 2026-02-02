//! Config key migration utilities with JSONC comment preservation.
//!
//! Responsibilities:
//! - Rename config keys while preserving JSONC comments and formatting.
//! - Check if config keys exist in project or global config files.
//! - Apply key renames safely with backup capability.
//!
//! Not handled here:
//! - Migration history tracking (see `history.rs`).
//! - File-level migrations like queue.json to queue.jsonc (see `file_migrations.rs`).
//!
//! Invariants/assumptions:
//! - Uses simple text replacement that preserves JSONC comments.
//! - Nested keys use dot notation (e.g., "agent.runner_cli").
//! - Both project and global configs are checked/updated.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::MigrationContext;

/// Check if a config key exists in either project or global config.
/// Supports dot notation for nested keys (e.g., "agent.runner_cli").
pub fn config_has_key(ctx: &MigrationContext, key: &str) -> bool {
    // Check project config first
    if let Ok(true) = config_file_has_key(&ctx.project_config_path, key) {
        return true;
    }

    // Check global config if available
    if let Some(global_path) = &ctx.global_config_path
        && let Ok(true) = config_file_has_key(global_path, key)
    {
        return true;
    }

    false
}

/// Check if a specific config file contains a key.
fn config_file_has_key(path: &Path, key: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;

    // Parse the JSONC to check for the key
    let value = match jsonc_parser::parse_to_serde_value(&raw, &Default::default()) {
        Ok(Some(v)) => v,
        _ => return Ok(false),
    };

    // Navigate to the key using dot notation
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &value;

    for part in &parts {
        match current {
            serde_json::Value::Object(map) => match map.get(*part) {
                Some(v) => current = v,
                None => return Ok(false),
            },
            _ => return Ok(false),
        }
    }

    Ok(true)
}

/// Apply a key rename to both project and global configs.
/// Uses text-based replacement to preserve comments.
pub fn apply_key_rename(ctx: &MigrationContext, old_key: &str, new_key: &str) -> Result<()> {
    // Update project config if it has the key
    if config_file_has_key(&ctx.project_config_path, old_key)? {
        rename_key_in_file(&ctx.project_config_path, old_key, new_key)
            .with_context(|| "rename key in project config".to_string())?;
    }

    // Update global config if it has the key
    if let Some(global_path) = &ctx.global_config_path
        && config_file_has_key(global_path, old_key)?
    {
        rename_key_in_file(global_path, old_key, new_key)
            .with_context(|| "rename key in global config".to_string())?;
    }

    Ok(())
}

/// Rename a key in a specific config file while preserving comments.
/// Uses text-based replacement for simplicity and reliability.
fn rename_key_in_file(path: &Path, old_key: &str, new_key: &str) -> Result<()> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;

    // Get the simple key name (last part of dot notation)
    let key_parts: Vec<&str> = old_key.split('.').collect();
    let target_key = key_parts
        .last()
        .ok_or_else(|| anyhow::anyhow!("Empty key"))?;

    // Perform the rename using text replacement
    let modified = rename_key_in_text(&raw, target_key, new_key)
        .with_context(|| format!("rename key {} to {} in text", old_key, new_key))?;

    // Write back the modified content
    crate::fsutil::write_atomic(path, modified.as_bytes())
        .with_context(|| format!("write modified config to {}", path.display()))?;

    log::info!(
        "Renamed config key '{}' to '{}' in {}",
        old_key,
        new_key,
        path.display()
    );

    Ok(())
}

/// Rename a key in JSONC text while preserving comments and formatting.
/// Uses regex-like pattern matching to find and replace key names.
fn rename_key_in_text(raw: &str, old_key: &str, new_key: &str) -> Result<String> {
    // Strategy: Find occurrences of the key pattern that look like JSON keys
    // A JSON key is: "key" or 'key' followed by optional whitespace and :

    let mut result = raw.to_string();

    // Pattern 1: Double-quoted key
    let double_quoted = format!(r#""{}""#, old_key);
    // Pattern 2: Single-quoted key (JSONC extension)
    let single_quoted = format!("'{}'", old_key);

    // Replace occurrences that are followed by optional whitespace and :
    result = replace_key_pattern(&result, &double_quoted, old_key, new_key);
    result = replace_key_pattern(&result, &single_quoted, old_key, new_key);

    Ok(result)
}

/// Replace key patterns that appear to be JSON object keys.
fn replace_key_pattern(text: &str, pattern: &str, old_key: &str, new_key: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;

    // Find all occurrences of the pattern
    for (start, _) in text.match_indices(pattern) {
        // Check if this looks like a key (followed by optional whitespace and :)
        let after_pattern = start + pattern.len();
        let rest = &text[after_pattern..];

        // Skip whitespace and check for colon
        let trimmed = rest.trim_start();
        let _whitespace_len = rest.len() - trimmed.len();

        if trimmed.starts_with(':') {
            // This looks like a JSON key, replace it
            result.push_str(&text[last_end..start + 1]); // Up to and including opening quote
            result.push_str(new_key); // New key name
            result.push_str(&text[start + 1 + old_key.len()..after_pattern]); // Closing quote
            last_end = after_pattern;
        }
    }

    // Append remaining text
    result.push_str(&text[last_end..]);

    result
}

/// Get the value of a config key from the context's resolved config.
/// Returns None if the key doesn't exist.
pub fn get_config_value(ctx: &MigrationContext, key: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = key.split('.').collect();

    // Convert config to serde_json::Value for easier navigation
    let config_json = match serde_json::to_value(&ctx.resolved_config) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let mut current = &config_json;
    for part in &parts {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get(*part)?;
            }
            _ => return None,
        }
    }

    Some(current.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn config_file_has_key_detects_existing_key() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "version": 1,
                "agent": {
                    "runner": "claude"
                }
            }"#,
        )
        .unwrap();

        assert!(config_file_has_key(&config_path, "version").unwrap());
        assert!(config_file_has_key(&config_path, "agent.runner").unwrap());
        assert!(!config_file_has_key(&config_path, "nonexistent").unwrap());
        assert!(!config_file_has_key(&config_path, "agent.nonexistent").unwrap());
    }

    #[test]
    fn config_file_has_key_handles_jsonc() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                // This is a comment
                "version": 1,
                "agent": {
                    "runner": "claude" // inline comment
                }
            }"#,
        )
        .unwrap();

        assert!(config_file_has_key(&config_path, "version").unwrap());
        assert!(config_file_has_key(&config_path, "agent.runner").unwrap());
    }

    #[test]
    fn rename_key_in_file_works_with_simple_key() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "version": 1,
                "old_key": "value"
            }"#,
        )
        .unwrap();

        rename_key_in_file(&config_path, "old_key", "new_key").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"new_key\""));
        assert!(!content.contains("\"old_key\""));
    }

    #[test]
    fn rename_key_preserves_comments() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                // Version comment
                "version": 1,
                /* Multi-line
                   comment */
                "old_key": "value"
            }"#,
        )
        .unwrap();

        rename_key_in_file(&config_path, "old_key", "new_key").unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("// Version comment"));
        assert!(content.contains("/* Multi-line"));
        assert!(content.contains("\"new_key\""));
        assert!(!content.contains("\"old_key\""));
    }

    #[test]
    fn rename_key_in_text_finds_quoted_key() {
        let raw = r#"{"version": 1, "old_key": "value"}"#;
        let result = rename_key_in_text(raw, "old_key", "new_key").unwrap();
        assert!(result.contains("\"new_key\""));
        assert!(!result.contains("\"old_key\""));
    }

    #[test]
    fn rename_key_in_text_preserves_non_key_occurrences() {
        // "old_key" appears as a value, not a key - should not be changed
        let raw = r#"{"key": "old_key", "old_key": "value"}"#;
        let result = rename_key_in_text(raw, "old_key", "new_key").unwrap();
        // The key should be renamed
        assert!(result.contains("\"new_key\": \"value\""));
        // The value should remain unchanged
        assert!(result.contains("\"key\": \"old_key\""));
    }

    #[test]
    fn rename_key_in_text_handles_whitespace() {
        let raw = r#"{
            "old_key"  : "value"
        }"#;
        let result = rename_key_in_text(raw, "old_key", "new_key").unwrap();
        assert!(result.contains("\"new_key\""));
        assert!(!result.contains("\"old_key\""));
    }
}
