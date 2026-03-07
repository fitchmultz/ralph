//! Config key migration utilities for rename/remove operations.
//!
//! Responsibilities:
//! - Rename config keys while preserving JSONC comments and formatting.
//! - Remove deprecated config keys from project/global config files.
//! - Check if config keys exist in project or global config files.
//! - Apply key renames safely with backup capability.
//!
//! Not handled here:
//! - Migration history tracking (see `history.rs`).
//! - File-level migrations like queue.json to queue.jsonc (see `file_migrations.rs`).
//!
//! Invariants/assumptions:
//! - Uses scoped text replacement that preserves JSONC comments.
//! - Nested keys use dot notation (e.g. "agent.runner_cli").
//! - Both project and global configs are checked/updated.
//! - Key renames are scoped to their parent object (e.g., "parallel.worktree_root"
//!   only renames "worktree_root" keys that appear within a "parallel" object).
//! - Key removals rewrite parsed JSON values and may normalize formatting/comments.

use anyhow::{Context, Result};
use serde_json::Value;
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

/// Apply a key removal to both project and global configs.
pub fn apply_key_remove(ctx: &MigrationContext, key: &str) -> Result<()> {
    // Update project config if it has the key
    if config_file_has_key(&ctx.project_config_path, key)? {
        remove_key_in_file(&ctx.project_config_path, key)
            .with_context(|| "remove key in project config".to_string())?;
    }

    // Update global config if it has the key
    if let Some(global_path) = &ctx.global_config_path
        && config_file_has_key(global_path, key)?
    {
        remove_key_in_file(global_path, key)
            .with_context(|| "remove key in global config".to_string())?;
    }

    Ok(())
}

/// Rewrite legacy CI gate keys into structured `agent.ci_gate` config.
pub fn apply_ci_gate_rewrite(ctx: &MigrationContext) -> Result<()> {
    rewrite_ci_gate_in_file(&ctx.project_config_path)?;

    if let Some(global_path) = &ctx.global_config_path {
        rewrite_ci_gate_in_file(global_path)?;
    }

    Ok(())
}

fn rewrite_ci_gate_in_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;
    let mut value = match jsonc_parser::parse_to_serde_value(&raw, &Default::default())? {
        Some(value) => value,
        None => return Ok(()),
    };

    let Some(root) = value.as_object_mut() else {
        return Ok(());
    };
    let Some(agent) = root.get_mut("agent").and_then(Value::as_object_mut) else {
        return Ok(());
    };

    let legacy_command = agent.remove("ci_gate_command");
    let legacy_enabled = agent.remove("ci_gate_enabled");
    if legacy_command.is_none() && legacy_enabled.is_none() {
        return Ok(());
    }

    let enabled = legacy_enabled
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let ci_gate = build_ci_gate_value(legacy_command.as_ref(), enabled)?;
    agent.insert("ci_gate".to_string(), ci_gate);

    let rendered = serde_json::to_string_pretty(&value).context("serialize migrated config")?;
    crate::fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write migrated config {}", path.display()))?;
    Ok(())
}

fn build_ci_gate_value(legacy_command: Option<&Value>, enabled: bool) -> Result<Value> {
    if !enabled {
        return Ok(serde_json::json!({ "enabled": false }));
    }

    let command = legacy_command
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("make ci");

    if contains_shell_syntax(command) {
        return Ok(serde_json::json!({
            "enabled": true,
            "shell": {
                "mode": if cfg!(windows) { "windows_cmd" } else { "posix" },
                "command": command,
            }
        }));
    }

    let argv = shlex::split(command).ok_or_else(|| {
        anyhow::anyhow!(
            "could not split legacy CI gate command into argv: {}",
            command
        )
    })?;
    if argv.is_empty() {
        return Ok(serde_json::json!({ "enabled": false }));
    }

    Ok(serde_json::json!({
        "enabled": true,
        "argv": argv,
    }))
}

fn contains_shell_syntax(command: &str) -> bool {
    ["&&", "||", "|", ";", "$(", "`", ">", "<"]
        .iter()
        .any(|needle| command.contains(needle))
}

/// Rename a key in a specific config file while preserving comments.
/// Uses scoped text-based replacement to only rename within the specified parent object.
/// For "parallel.worktree_root", only renames "worktree_root" inside "parallel" objects.
fn rename_key_in_file(path: &Path, old_key: &str, new_key: &str) -> Result<()> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;

    // Parse the dot-notation keys to extract parent path and leaf names
    let old_parts: Vec<&str> = old_key.split('.').collect();
    let new_parts: Vec<&str> = new_key.split('.').collect();

    if old_parts.is_empty() || new_parts.is_empty() {
        return Err(anyhow::anyhow!("Empty key"));
    }

    // Extract leaf key names (last segment)
    let old_leaf = old_parts[old_parts.len() - 1];
    let new_leaf = new_parts[new_parts.len() - 1];

    // Extract parent path (all segments except last) for scoping
    let parent_path = if old_parts.len() > 1 {
        old_parts[..old_parts.len() - 1].to_vec()
    } else {
        Vec::new()
    };

    // Perform the scoped rename
    let modified = if parent_path.is_empty() {
        // No parent scope - rename all occurrences of the leaf key
        rename_key_in_text(&raw, old_leaf, new_leaf)
    } else {
        // Scoped rename - only rename within the specified parent object
        rename_key_in_text_scoped(&raw, &parent_path, old_leaf, new_leaf)
    }
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

/// Remove a key from a specific config file.
fn remove_key_in_file(path: &Path, key: &str) -> Result<()> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;

    let mut value = jsonc_parser::parse_to_serde_value(&raw, &Default::default())?
        .ok_or_else(|| anyhow::anyhow!("parse config file {}", path.display()))?;

    remove_key_from_value(&mut value, key);

    let modified = serde_json::to_string_pretty(&value).context("serialize config")?;
    crate::fsutil::write_atomic(path, modified.as_bytes())
        .with_context(|| format!("write modified config to {}", path.display()))?;

    log::info!("Removed config key '{}' in {}", key, path.display());
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

/// Rename a key within a scoped parent object path.
/// For example, with parent_path=["parallel"], old_key="worktree_root",
/// only renames "worktree_root" keys that appear inside "parallel" objects.
fn rename_key_in_text_scoped(
    raw: &str,
    parent_path: &[&str],
    old_key: &str,
    new_key: &str,
) -> Result<String> {
    // Parse the JSONC to understand structure
    let value = match jsonc_parser::parse_to_serde_value(raw, &Default::default()) {
        Ok(Some(v)) => v,
        _ => {
            // If we can't parse, fall back to global rename
            return rename_key_in_text(raw, old_key, new_key);
        }
    };

    // Check if the key exists at the specified path
    if !key_exists_at_path(&value, parent_path, old_key) {
        // Key doesn't exist at this path, return unchanged
        return Ok(raw.to_string());
    }

    // Find the parent object in the raw text and rename only within its scope
    let parent_key = parent_path[0]; // We only support single-level nesting for now
    rename_key_in_object_scope(raw, parent_key, old_key, new_key)
}

/// Check if a key exists at a specific nested path in the JSON value.
fn key_exists_at_path(value: &serde_json::Value, path: &[&str], key: &str) -> bool {
    let mut current = value;

    for part in path {
        match current {
            serde_json::Value::Object(map) => {
                if let Some(v) = map.get(*part) {
                    current = v;
                } else {
                    return false;
                }
            }
            _ => return false,
        }
    }

    match current {
        serde_json::Value::Object(map) => map.contains_key(key),
        _ => false,
    }
}

/// Rename a key within a specific object scope in the raw text.
/// Finds the object by its key and renames the target key only within that object's scope.
fn rename_key_in_object_scope(
    raw: &str,
    object_key: &str,
    old_key: &str,
    new_key: &str,
) -> Result<String> {
    // Pattern to find the object: "object_key" followed by optional whitespace, :, optional whitespace, and {
    let object_pattern = format!(r#""{}""#, object_key);

    let mut result = String::with_capacity(raw.len());
    let mut last_end = 0;

    // Find all occurrences of the object key
    for (start, _) in raw.match_indices(&object_pattern) {
        // Check if this looks like an object key (followed by optional whitespace, :, optional whitespace, and {)
        let after_pattern = start + object_pattern.len();
        let rest = &raw[after_pattern..];

        // Skip whitespace
        let rest_trimmed = rest.trim_start();
        let whitespace_before_colon = rest.len() - rest_trimmed.len();

        // Check for colon
        if !rest_trimmed.starts_with(':') {
            continue;
        }

        // Skip colon and whitespace after it
        let after_colon = &rest_trimmed[1..];
        let after_colon_trimmed = after_colon.trim_start();
        let whitespace_after_colon = after_colon.len() - after_colon_trimmed.len();

        // Check for opening brace
        if !after_colon_trimmed.starts_with('{') {
            continue;
        }

        // Found the target object - calculate positions
        // object_content_start points to the '{'
        let object_content_start =
            after_pattern + whitespace_before_colon + 1 + whitespace_after_colon;

        // Find the end of this object by tracking brace depth
        let after_brace = object_content_start + 1;
        let mut pos = after_brace;
        let mut depth = 1;

        while pos < raw.len() && depth > 0 {
            match raw.as_bytes().get(pos) {
                Some(b'{') => depth += 1,
                Some(b'}') => depth -= 1,
                Some(b'"') => {
                    // Skip string literals
                    pos += 1;
                    while pos < raw.len() {
                        match raw.as_bytes().get(pos) {
                            Some(b'\\') => pos += 2,
                            Some(b'"') => {
                                pos += 1;
                                break;
                            }
                            _ => pos += 1,
                        }
                    }
                    continue;
                }
                _ => {}
            }
            pos += 1;
        }

        let object_content_end = pos; // Position after closing '}'

        // Add text before the object content (including the key, colon, and opening brace)
        result.push_str(&raw[last_end..object_content_start]);

        // Process only the content inside the braces to rename the key
        let inner_content = &raw[object_content_start..object_content_end];
        let modified_inner = rename_key_in_text(inner_content, old_key, new_key)?;
        result.push_str(&modified_inner);

        last_end = object_content_end;
    }

    // Append remaining text
    result.push_str(&raw[last_end..]);

    Ok(result)
}

/// Remove a key from a serde_json value using dot notation (e.g., "agent.runner").
fn remove_key_from_value(value: &mut serde_json::Value, key: &str) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return;
    }

    let mut current = value;
    for part in &parts[..parts.len() - 1] {
        match current {
            serde_json::Value::Object(map) => {
                if let Some(next) = map.get_mut(*part) {
                    current = next;
                } else {
                    return;
                }
            }
            _ => return,
        }
    }

    if let serde_json::Value::Object(map) = current {
        map.remove(parts[parts.len() - 1]);
    }
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

    #[test]
    fn rename_key_in_file_uses_leaf_of_dot_path_keys() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        fs::write(&config_path, r#"{"parallel":{"worktree_root":"x"}}"#).unwrap();

        rename_key_in_file(
            &config_path,
            "parallel.worktree_root",
            "parallel.workspace_root",
        )
        .unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"workspace_root\""));
        assert!(!content.contains("\"worktree_root\""));
        assert!(!content.contains("\"parallel.workspace_root\""));
    }

    #[test]
    fn rename_key_scoped_to_parent_object() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        // Config with worktree_root in both "parallel" and "other" objects
        fs::write(
            &config_path,
            r#"{
                "parallel": {
                    "worktree_root": "/tmp/parallel"
                },
                "other": {
                    "worktree_root": "/tmp/other"
                }
            }"#,
        )
        .unwrap();

        rename_key_in_file(
            &config_path,
            "parallel.worktree_root",
            "parallel.workspace_root",
        )
        .unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        // parallel.worktree_root should be renamed
        assert!(content.contains(
            "\"parallel\": {\n                    \"workspace_root\": \"/tmp/parallel\""
        ));
        // other.worktree_root should NOT be renamed
        assert!(
            content.contains("\"other\": {\n                    \"worktree_root\": \"/tmp/other\"")
        );
    }

    #[test]
    fn rename_key_scoped_with_comments() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                // Parallel execution settings
                "parallel": {
                    /* old setting name */
                    "worktree_root": "/tmp/worktrees"
                }
            }"#,
        )
        .unwrap();

        rename_key_in_file(
            &config_path,
            "parallel.worktree_root",
            "parallel.workspace_root",
        )
        .unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"workspace_root\": \"/tmp/worktrees\""));
        assert!(!content.contains("\"worktree_root\""));
        // Comments should be preserved
        assert!(content.contains("// Parallel execution settings"));
        assert!(content.contains("/* old setting name */"));
    }

    #[test]
    fn remove_key_in_file_removes_nested_key() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "version": 1,
                "agent": {
                    "runner": "claude",
                    "update_task_before_run": true
                }
            }"#,
        )
        .unwrap();

        remove_key_in_file(&config_path, "agent.update_task_before_run").unwrap();

        let value = jsonc_parser::parse_to_serde_value(
            &fs::read_to_string(&config_path).unwrap(),
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        let agent = value.get("agent").unwrap();
        assert!(agent.get("update_task_before_run").is_none());
        assert_eq!(agent.get("runner").and_then(|v| v.as_str()), Some("claude"));
    }
}
