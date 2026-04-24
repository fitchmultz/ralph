//! Unknown config key detection and handling.
//!
//! Purpose:
//! - Unknown config key detection and handling.
//!
//! Responsibilities:
//! - Detect unknown keys in config files (project and global)
//! - Prompt user for action (remove/keep/rename) or auto-remove
//! - Manipulate JSON config files to remove or rename keys
//! - Extract known keys from schema for comparison
//!
//! Not handled here:
//! - README updates (see readme.rs)
//! - Config migrations (see migrations.rs)
//! - Schema definition (see contracts/)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Unknown keys are detected by comparing against schemars-generated schema
//! - Auto-fix removes unknown keys without prompting
//! - Non-interactive mode without auto-fix keeps keys with warning

use crate::config::Resolved;
use crate::migration::config_migrations::{remove_key_in_file, rename_key_in_file};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::io::{self, Write};

/// Action to take for an unknown key.
#[derive(Debug, Clone)]
enum UnknownKeyAction {
    /// Remove the unknown key.
    Remove,
    /// Keep the unknown key as-is.
    Keep,
    /// Rename the key to a new name.
    Rename(String),
}

/// Check for unknown config keys in config files.
///
/// Returns a list of actions taken.
pub(crate) fn check_unknown_keys(
    resolved: &Resolved,
    auto_fix: bool,
    non_interactive: bool,
    can_prompt: impl Fn() -> bool,
) -> Result<Vec<String>> {
    let mut actions = Vec::new();
    let known_keys = get_known_config_keys();

    // Check project config
    if let Some(ref project_path) = resolved.project_config_path
        && project_path.exists()
    {
        match check_config_file_unknown_keys(project_path, &known_keys) {
            Ok(unknown_keys) => {
                for key in unknown_keys {
                    let action = determine_key_action(
                        &key,
                        "project config",
                        auto_fix,
                        non_interactive,
                        &can_prompt,
                    )?;
                    actions.extend(apply_key_action(
                        project_path,
                        &key,
                        action,
                        "project config",
                    )?);
                }
            }
            Err(e) => {
                log::warn!("Failed to check project config for unknown keys: {}", e);
            }
        }
    }

    // Check global config
    if let Some(ref global_path) = resolved.global_config_path
        && global_path.exists()
    {
        match check_config_file_unknown_keys(global_path, &known_keys) {
            Ok(unknown_keys) => {
                for key in unknown_keys {
                    let action = determine_key_action(
                        &key,
                        "global config",
                        auto_fix,
                        non_interactive,
                        &can_prompt,
                    )?;
                    actions.extend(apply_key_action(
                        global_path,
                        &key,
                        action,
                        "global config",
                    )?);
                }
            }
            Err(e) => {
                log::warn!("Failed to check global config for unknown keys: {}", e);
            }
        }
    }

    Ok(actions)
}

fn determine_key_action(
    key: &str,
    config_file: &str,
    auto_fix: bool,
    non_interactive: bool,
    can_prompt: &impl Fn() -> bool,
) -> Result<UnknownKeyAction> {
    if auto_fix {
        return Ok(UnknownKeyAction::Remove);
    }

    if !non_interactive && can_prompt() {
        prompt_unknown_key(key, config_file)
    } else {
        log::warn!(
            "Unknown config key '{}' in {} (use --auto-fix to remove)",
            key,
            config_file
        );
        Ok(UnknownKeyAction::Keep)
    }
}

fn apply_key_action(
    path: &std::path::Path,
    key: &str,
    action: UnknownKeyAction,
    config_file: &str,
) -> Result<Vec<String>> {
    let mut actions = Vec::new();
    match action {
        UnknownKeyAction::Remove => match remove_key_in_file(path, key) {
            Ok(()) => {
                actions.push(format!(
                    "Removed unknown key '{}' from {}",
                    key, config_file
                ));
            }
            Err(e) => {
                log::warn!("Failed to remove key '{}': {}", key, e);
            }
        },
        UnknownKeyAction::Keep => {
            log::info!("Kept unknown key '{}' in {}", key, config_file);
        }
        UnknownKeyAction::Rename(new_key) => match rename_key_in_file(path, key, &new_key) {
            Ok(()) => {
                actions.push(format!(
                    "Renamed key '{}' to '{}' in {}",
                    key, new_key, config_file
                ));
            }
            Err(e) => {
                log::warn!("Failed to rename key '{}': {}", key, e);
            }
        },
    }
    Ok(actions)
}

/// Prompt user for action on an unknown key.
fn prompt_unknown_key(key: &str, config_file: &str) -> Result<UnknownKeyAction> {
    print!(
        "Unknown config key '{}' in {}. [r]emove, [k]eep, or rename to: ",
        key, config_file
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(parse_unknown_key_action(&input))
}

fn parse_unknown_key_action(input: &str) -> UnknownKeyAction {
    let trimmed = input.trim();
    let command = trimmed.to_ascii_lowercase();

    if trimmed.is_empty() || command == "k" || command == "keep" {
        UnknownKeyAction::Keep
    } else if command == "r" || command == "remove" {
        UnknownKeyAction::Remove
    } else {
        UnknownKeyAction::Rename(trimmed.to_string())
    }
}

/// Get the set of known config keys from the Config schema.
fn get_known_config_keys() -> HashSet<String> {
    use serde_json::Value;

    let schema = schemars::schema_for!(crate::contracts::Config);
    let mut keys = HashSet::new();
    let Some(root) = schema.as_object() else {
        return keys;
    };

    if let Some(properties) = root.get("properties").and_then(Value::as_object) {
        let definitions = root
            .get("$defs")
            .and_then(Value::as_object)
            .or_else(|| root.get("definitions").and_then(Value::as_object));
        for (key, subschema) in properties {
            keys.insert(key.clone());
            extract_keys_from_schema(subschema, key, &mut keys, definitions);
        }
    }

    keys
}

/// Recursively extract dot-notation keys from a schema.
fn extract_keys_from_schema(
    schema: &serde_json::Value,
    prefix: &str,
    keys: &mut HashSet<String>,
    definitions: Option<&serde_json::Map<String, serde_json::Value>>,
) {
    use serde_json::Value;

    let Some(obj) = schema.as_object() else {
        return;
    };

    if let Some(ref_path) = obj.get("$ref").and_then(Value::as_str) {
        if let Some(definitions) = definitions
            && let Some(def_name) = ref_path
                .strip_prefix("#/$defs/")
                .or_else(|| ref_path.strip_prefix("#/definitions/"))
            && let Some(def_schema) = definitions.get(def_name)
        {
            extract_keys_from_schema(def_schema, prefix, keys, Some(definitions));
        }
        return;
    }

    if let Some(properties) = obj.get("properties").and_then(Value::as_object) {
        for (key, subschema) in properties {
            let full_key = format!("{}.{}", prefix, key);
            keys.insert(full_key.clone());
            extract_keys_from_schema(subschema, &full_key, keys, definitions);
        }
    }

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(subschemas) = obj.get(keyword).and_then(Value::as_array) {
            for sub in subschemas {
                extract_keys_from_schema(sub, prefix, keys, definitions);
            }
        }
    }
}

/// Check a config file for unknown keys.
fn check_config_file_unknown_keys(
    path: &std::path::Path,
    known_keys: &HashSet<String>,
) -> Result<Vec<String>> {
    use std::fs;

    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    let value =
        match jsonc_parser::parse_to_serde_value::<serde_json::Value>(&raw, &Default::default()) {
            Ok(v) => v,
            Err(_) => return Ok(Vec::new()),
        };

    let mut unknown_keys = Vec::new();
    collect_unknown_keys(&value, known_keys, "", &mut unknown_keys);

    Ok(unknown_keys)
}

/// Recursively collect unknown keys from a JSON value.
fn collect_unknown_keys(
    value: &serde_json::Value,
    known_keys: &HashSet<String>,
    prefix: &str,
    unknown: &mut Vec<String>,
) {
    if let serde_json::Value::Object(map) = value {
        for (key, child) in map {
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            if !known_keys.contains(&full_key)
                && !is_known_parent_key(&full_key, known_keys)
                && (!child.is_object() || child.as_object().map(|m| m.is_empty()).unwrap_or(false))
            {
                unknown.push(full_key.clone());
            }

            collect_unknown_keys(child, known_keys, &full_key, unknown);
        }
    }
}

/// Check if a key is a known parent key.
fn is_known_parent_key(key: &str, known_keys: &HashSet<String>) -> bool {
    for known in known_keys {
        if known.starts_with(&format!("{}.", key)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
