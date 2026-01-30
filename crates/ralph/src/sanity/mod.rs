//! Automatic startup health checks with auto-fix and migration prompts.
//!
//! Responsibilities:
//! - Run lightweight health checks on Ralph startup for key commands.
//! - Auto-update README.md when embedded template is newer (no prompt).
//! - Detect and prompt for config migrations (deprecated keys, unknown keys).
//! - Support --auto-fix flag to auto-approve all migrations without prompting.
//! - Support --no-sanity-checks flag to skip all health checks.
//!
//! Not handled here:
//! - Deep validation (git, runners, queue structure) - that's `ralph doctor`.
//! - Interactive TUI flows.
//! - Network connectivity checks.
//!
//! Invariants/assumptions:
//! - Sanity checks are fast and lightweight.
//! - README auto-update is automatic (users shouldn't edit this file manually).
//! - Config migrations require user confirmation unless --auto-fix is set.
//! - Unknown config keys prompt for remove/keep/rename action.
//! - If stdin is not a TTY and --auto-fix is not set, warn and skip interactive prompts.

use crate::config::Resolved;
use crate::migration::MigrationContext;
use crate::outpututil;
use anyhow::{Context, Result};
use std::io::{self, Write};

/// Options for controlling sanity check behavior.
#[derive(Debug, Clone, Default)]
pub struct SanityOptions {
    /// Auto-approve all fixes without prompting.
    pub auto_fix: bool,
    /// Skip all sanity checks.
    pub skip: bool,
}

/// Result of running sanity checks.
#[derive(Debug, Clone, Default)]
pub struct SanityResult {
    /// Fixes that were automatically applied.
    pub auto_fixes: Vec<String>,
    /// Issues that need user attention (could not be auto-fixed).
    pub needs_attention: Vec<SanityIssue>,
}

/// A single issue found during sanity checks.
#[derive(Debug, Clone)]
pub struct SanityIssue {
    /// Severity of the issue.
    pub severity: IssueSeverity,
    /// Human-readable description of the issue.
    pub message: String,
    /// Whether a fix is available for this issue.
    pub fix_available: bool,
}

/// Severity level for sanity issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Warning - operation can continue.
    Warning,
    /// Error - operation should not proceed.
    Error,
}

/// Run all sanity checks and apply fixes based on options.
///
/// This is the main entry point for sanity checks. It:
/// 1. Checks and auto-updates README if needed (no prompt).
/// 2. Checks for pending config migrations and prompts/apply.
/// 3. Checks for unknown config keys and prompts for action.
///
/// Returns a `SanityResult` describing what was fixed and what needs attention.
pub fn run_sanity_checks(resolved: &Resolved, options: &SanityOptions) -> Result<SanityResult> {
    if options.skip {
        log::debug!("Sanity checks skipped via --no-sanity-checks");
        return Ok(SanityResult::default());
    }

    log::debug!("Running sanity checks...");
    let mut result = SanityResult::default();

    // Check 1: README auto-update (automatic, no prompt)
    match check_and_update_readme(resolved) {
        Ok(Some(fix_msg)) => {
            result.auto_fixes.push(fix_msg);
        }
        Ok(None) => {
            log::debug!("README is current");
        }
        Err(e) => {
            log::warn!("Failed to check/update README: {}", e);
            result.needs_attention.push(SanityIssue {
                severity: IssueSeverity::Warning,
                message: format!("README check failed: {}", e),
                fix_available: false,
            });
        }
    }

    // Check 2: Config migrations (prompt unless auto_fix)
    let mut ctx = match MigrationContext::from_resolved(resolved) {
        Ok(ctx) => ctx,
        Err(e) => {
            log::warn!("Failed to create migration context: {}", e);
            result.needs_attention.push(SanityIssue {
                severity: IssueSeverity::Warning,
                message: format!("Config migration check failed: {}", e),
                fix_available: false,
            });
            return Ok(result);
        }
    };

    match check_and_handle_migrations(&mut ctx, options.auto_fix) {
        Ok(migration_fixes) => {
            result.auto_fixes.extend(migration_fixes);
        }
        Err(e) => {
            log::warn!("Migration handling failed: {}", e);
            result.needs_attention.push(SanityIssue {
                severity: IssueSeverity::Warning,
                message: format!("Migration handling failed: {}", e),
                fix_available: false,
            });
        }
    }

    // Check 3: Unknown config keys
    match check_unknown_keys(resolved, options.auto_fix) {
        Ok(unknown_fixes) => {
            result.auto_fixes.extend(unknown_fixes);
        }
        Err(e) => {
            log::warn!("Unknown key check failed: {}", e);
            result.needs_attention.push(SanityIssue {
                severity: IssueSeverity::Warning,
                message: format!("Unknown key check failed: {}", e),
                fix_available: false,
            });
        }
    }

    // Report results
    if !result.auto_fixes.is_empty() {
        log::info!("Applied {} automatic fix(es):", result.auto_fixes.len());
        for fix in &result.auto_fixes {
            outpututil::log_success(&format!("  - {}", fix));
        }
    }

    if !result.needs_attention.is_empty() {
        log::warn!(
            "Found {} issue(s) needing attention:",
            result.needs_attention.len()
        );
        for issue in &result.needs_attention {
            match issue.severity {
                IssueSeverity::Warning => outpututil::log_warn(&format!("  - {}", issue.message)),
                IssueSeverity::Error => outpututil::log_error(&format!("  - {}", issue.message)),
            }
        }
    }

    log::debug!("Sanity checks complete");
    Ok(result)
}

/// Check and auto-update README if needed.
///
/// Returns `Ok(Some(message))` if README was updated.
/// Returns `Ok(None)` if README is current or not applicable.
fn check_and_update_readme(resolved: &Resolved) -> Result<Option<String>> {
    use crate::commands::init::readme;

    match readme::check_readme_current(resolved)? {
        readme::ReadmeCheckResult::Current(version) => {
            log::debug!("README is current (version {})", version);
            Ok(None)
        }
        readme::ReadmeCheckResult::Outdated {
            current_version,
            embedded_version,
        } => {
            let readme_path = resolved.repo_root.join(".ralph/README.md");
            log::info!(
                "README is outdated (version {} < {}), updating...",
                current_version,
                embedded_version
            );

            let (status, _) =
                readme::write_readme(&readme_path, false, true).context("write updated README")?;

            if status == crate::commands::init::FileInitStatus::Updated {
                let msg = format!(
                    "Updated README from version {} to {}",
                    current_version, embedded_version
                );
                log::info!("{}", msg);
                Ok(Some(msg))
            } else {
                // This shouldn't happen, but handle gracefully
                log::debug!("README write returned status: {:?}", status);
                Ok(None)
            }
        }
        readme::ReadmeCheckResult::Missing => {
            log::debug!("README.md is missing (optional)");
            Ok(None)
        }
        readme::ReadmeCheckResult::NotApplicable => {
            log::debug!("README.md is not applicable");
            Ok(None)
        }
    }
}

/// Check for pending config migrations and prompt/apply them.
///
/// Returns a list of migration descriptions that were applied.
fn check_and_handle_migrations(ctx: &mut MigrationContext, auto_fix: bool) -> Result<Vec<String>> {
    use crate::migration::{
        apply_migration, check_migrations, MigrationCheckResult, MigrationType,
    };

    let mut applied = Vec::new();

    match check_migrations(ctx)? {
        MigrationCheckResult::Current => {
            log::debug!("No pending config migrations");
        }
        MigrationCheckResult::Pending(migrations) => {
            log::info!("Found {} pending config migration(s)", migrations.len());

            for migration in migrations {
                let description = match &migration.migration_type {
                    MigrationType::ConfigKeyRename { old_key, new_key } => {
                        format!(
                            "Config uses deprecated key '{}', migrate to '{}'",
                            old_key, new_key
                        )
                    }
                    MigrationType::FileRename { old_path, new_path } => {
                        format!("Rename file '{}' to '{}'", old_path, new_path)
                    }
                    MigrationType::ReadmeUpdate {
                        from_version,
                        to_version,
                    } => {
                        format!(
                            "Update README from version {} to {}",
                            from_version, to_version
                        )
                    }
                };

                let should_apply = if auto_fix {
                    true
                } else if is_tty() {
                    // Prompt user
                    prompt_yes_no(&description, true)?
                } else {
                    // Not a TTY, can't prompt
                    log::warn!("{} (use --auto-fix to apply)", description);
                    false
                };

                if should_apply {
                    apply_migration(ctx, migration)
                        .with_context(|| format!("apply migration {}", migration.id))?;
                    applied.push(format!("Applied: {}", migration.description));
                } else {
                    log::info!("Skipped migration: {}", migration.description);
                }
            }
        }
    }

    Ok(applied)
}

/// Check for unknown config keys in config files.
///
/// This checks both project and global config files for keys that are not
/// recognized by the schema. Unknown keys are reported and can be removed
/// or kept based on user input (or auto-fix setting).
///
/// Returns a list of actions taken.
fn check_unknown_keys(resolved: &Resolved, auto_fix: bool) -> Result<Vec<String>> {
    let mut actions = Vec::new();

    // Get known keys from the ConfigLayer struct
    let known_keys = get_known_config_keys();

    // Check project config
    if let Some(ref project_path) = resolved.project_config_path {
        if project_path.exists() {
            match check_config_file_unknown_keys(project_path, &known_keys) {
                Ok(unknown_keys) => {
                    for key in unknown_keys {
                        let action = if auto_fix {
                            // Auto-fix: remove unknown keys
                            match remove_key_from_config_file(project_path, &key) {
                                Ok(()) => {
                                    actions.push(format!(
                                        "Removed unknown key '{}' from project config",
                                        key
                                    ));
                                    continue;
                                }
                                Err(e) => {
                                    log::warn!("Failed to remove key '{}': {}", key, e);
                                    UnknownKeyAction::Keep
                                }
                            }
                        } else if is_tty() {
                            // Interactive: prompt user
                            prompt_unknown_key(&key, "project config")?
                        } else {
                            // Not a TTY: warn and keep
                            log::warn!(
                                "Unknown config key '{}' in project config (use --auto-fix to remove)",
                                key
                            );
                            UnknownKeyAction::Keep
                        };

                        match action {
                            UnknownKeyAction::Remove => {
                                match remove_key_from_config_file(project_path, &key) {
                                    Ok(()) => {
                                        actions.push(format!(
                                            "Removed unknown key '{}' from project config",
                                            key
                                        ));
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to remove key '{}': {}", key, e);
                                    }
                                }
                            }
                            UnknownKeyAction::Keep => {
                                log::info!("Kept unknown key '{}' in project config", key);
                            }
                            UnknownKeyAction::Rename(new_key) => {
                                match rename_key_in_config_file(project_path, &key, &new_key) {
                                    Ok(()) => {
                                        actions.push(format!(
                                            "Renamed key '{}' to '{}' in project config",
                                            key, new_key
                                        ));
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to rename key '{}': {}", key, e);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to check project config for unknown keys: {}", e);
                }
            }
        }
    }

    // Check global config
    if let Some(ref global_path) = resolved.global_config_path {
        if global_path.exists() {
            match check_config_file_unknown_keys(global_path, &known_keys) {
                Ok(unknown_keys) => {
                    for key in unknown_keys {
                        let action = if auto_fix {
                            // Auto-fix: remove unknown keys
                            match remove_key_from_config_file(global_path, &key) {
                                Ok(()) => {
                                    actions.push(format!(
                                        "Removed unknown key '{}' from global config",
                                        key
                                    ));
                                    continue;
                                }
                                Err(e) => {
                                    log::warn!("Failed to remove key '{}': {}", key, e);
                                    UnknownKeyAction::Keep
                                }
                            }
                        } else if is_tty() {
                            // Interactive: prompt user
                            prompt_unknown_key(&key, "global config")?
                        } else {
                            // Not a TTY: warn and keep
                            log::warn!(
                                "Unknown config key '{}' in global config (use --auto-fix to remove)",
                                key
                            );
                            UnknownKeyAction::Keep
                        };

                        match action {
                            UnknownKeyAction::Remove => {
                                match remove_key_from_config_file(global_path, &key) {
                                    Ok(()) => {
                                        actions.push(format!(
                                            "Removed unknown key '{}' from global config",
                                            key
                                        ));
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to remove key '{}': {}", key, e);
                                    }
                                }
                            }
                            UnknownKeyAction::Keep => {
                                log::info!("Kept unknown key '{}' in global config", key);
                            }
                            UnknownKeyAction::Rename(new_key) => {
                                match rename_key_in_config_file(global_path, &key, &new_key) {
                                    Ok(()) => {
                                        actions.push(format!(
                                            "Renamed key '{}' to '{}' in global config",
                                            key, new_key
                                        ));
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to rename key '{}': {}", key, e);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to check global config for unknown keys: {}", e);
                }
            }
        }
    }

    Ok(actions)
}

/// Get the set of known config keys from the Config schema.
///
/// This extracts keys dynamically from the schemars-generated schema,
/// ensuring the key list stays in sync with the actual Config struct.
fn get_known_config_keys() -> std::collections::HashSet<String> {
    use schemars::schema::RootSchema;
    use std::collections::HashSet;

    let schema: RootSchema = schemars::schema_for!(crate::contracts::Config);
    let mut keys = HashSet::new();

    // Add top-level keys and recurse into their schemas to resolve refs.
    if let Some(object) = &schema.schema.object {
        for (key, subschema) in &object.properties {
            keys.insert(key.clone());
            extract_keys_from_schema(subschema, key, &mut keys, &schema.definitions);
        }
    }

    keys
}

/// Recursively extract dot-notation keys from a schema.
fn extract_keys_from_schema(
    schema: &schemars::schema::Schema,
    prefix: &str,
    keys: &mut std::collections::HashSet<String>,
    definitions: &schemars::Map<String, schemars::schema::Schema>,
) {
    use schemars::schema::Schema;

    // Unwrap the schema object (skip boolean schemas)
    let obj = match schema {
        Schema::Object(obj) => obj,
        Schema::Bool(_) => return,
    };

    // Follow references to definitions (e.g., "#/definitions/NotificationConfig")
    if let Some(ref_path) = &obj.reference {
        if let Some(def_name) = ref_path.strip_prefix("#/definitions/") {
            if let Some(def_schema) = definitions.get(def_name) {
                extract_keys_from_schema(def_schema, prefix, keys, definitions);
            }
        }
        return;
    }

    // Process object properties
    if let Some(object) = &obj.object {
        for (key, subschema) in &object.properties {
            let full_key = format!("{}.{}", prefix, key);
            keys.insert(full_key.clone());
            extract_keys_from_schema(subschema, &full_key, keys, definitions);
        }
    }

    // Process subschemas (allOf, anyOf, oneOf)
    if let Some(subschemas) = &obj.subschemas {
        if let Some(all_of) = &subschemas.all_of {
            for sub in all_of {
                extract_keys_from_schema(sub, prefix, keys, definitions);
            }
        }
        if let Some(any_of) = &subschemas.any_of {
            for sub in any_of {
                extract_keys_from_schema(sub, prefix, keys, definitions);
            }
        }
        if let Some(one_of) = &subschemas.one_of {
            for sub in one_of {
                extract_keys_from_schema(sub, prefix, keys, definitions);
            }
        }
    }
}

/// Check a config file for unknown keys.
fn check_config_file_unknown_keys(
    path: &std::path::Path,
    known_keys: &std::collections::HashSet<String>,
) -> Result<Vec<String>> {
    use std::fs;

    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    // Parse the JSONC to get all keys
    let value = match jsonc_parser::parse_to_serde_value(&raw, &Default::default()) {
        Ok(Some(v)) => v,
        _ => return Ok(Vec::new()), // Can't parse, assume no unknown keys
    };

    let mut unknown_keys = Vec::new();
    collect_unknown_keys(&value, known_keys, "", &mut unknown_keys);

    Ok(unknown_keys)
}

/// Recursively collect unknown keys from a JSON value.
fn collect_unknown_keys(
    value: &serde_json::Value,
    known_keys: &std::collections::HashSet<String>,
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

            // Check if this key is known
            if !known_keys.contains(&full_key) && !is_known_parent_key(&full_key, known_keys) {
                // Only report leaf keys (non-objects or empty objects)
                if !child.is_object() || child.as_object().map(|m| m.is_empty()).unwrap_or(false) {
                    unknown.push(full_key.clone());
                }
            }

            // Recurse into child objects
            collect_unknown_keys(child, known_keys, &full_key, unknown);
        }
    }
}

/// Check if a key is a known parent key (i.e., it's a prefix of known keys).
fn is_known_parent_key(key: &str, known_keys: &std::collections::HashSet<String>) -> bool {
    for known in known_keys {
        if known.starts_with(&format!("{}.", key)) {
            return true;
        }
    }
    false
}

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

/// Prompt user for action on an unknown key.
fn prompt_unknown_key(key: &str, config_file: &str) -> Result<UnknownKeyAction> {
    print!(
        "Unknown config key '{}' in {}. [r]emove, [k]eep, or rename to: ",
        key, config_file
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();

    if trimmed.is_empty() || trimmed == "k" || trimmed == "keep" {
        Ok(UnknownKeyAction::Keep)
    } else if trimmed == "r" || trimmed == "remove" {
        Ok(UnknownKeyAction::Remove)
    } else if !trimmed.is_empty() {
        // Treat as rename target
        Ok(UnknownKeyAction::Rename(trimmed))
    } else {
        Ok(UnknownKeyAction::Keep)
    }
}

/// Remove a key from a config file.
fn remove_key_from_config_file(path: &std::path::Path, key: &str) -> Result<()> {
    use std::fs;

    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    // Parse and remove the key
    let mut value: serde_json::Value =
        jsonc_parser::parse_to_serde_value(&raw, &Default::default())
            .context("parse config")?
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    remove_key_from_value(&mut value, key);

    // Write back
    let modified = serde_json::to_string_pretty(&value).context("serialize config")?;
    crate::fsutil::write_atomic(path, modified.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;

    log::info!("Removed key '{}' from {}", key, path.display());
    Ok(())
}

/// Remove a key from a JSON value using dot notation.
fn remove_key_from_value(value: &mut serde_json::Value, key: &str) {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return;
    }

    if parts.len() == 1 {
        // Direct removal
        if let serde_json::Value::Object(map) = value {
            map.remove(parts[0]);
        }
    } else {
        // Navigate to parent and remove
        let parent_key = parts[..parts.len() - 1].join(".");
        let child_key = parts[parts.len() - 1];

        if let Some(serde_json::Value::Object(map)) = get_nested_value_mut(value, &parent_key) {
            map.remove(child_key);
        }
    }
}

/// Get a mutable reference to a nested value.
fn get_nested_value_mut<'a>(
    value: &'a mut serde_json::Value,
    key: &str,
) -> Option<&'a mut serde_json::Value> {
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = value;

    for part in parts {
        match current {
            serde_json::Value::Object(map) => {
                current = map.get_mut(part)?;
            }
            _ => return None,
        }
    }

    Some(current)
}

/// Rename a key in a config file.
fn rename_key_in_config_file(path: &std::path::Path, old_key: &str, new_key: &str) -> Result<()> {
    use std::fs;

    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    // Parse and rename the key
    let mut value: serde_json::Value =
        jsonc_parser::parse_to_serde_value(&raw, &Default::default())
            .context("parse config")?
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    rename_key_in_value(&mut value, old_key, new_key);

    // Write back
    let modified = serde_json::to_string_pretty(&value).context("serialize config")?;
    crate::fsutil::write_atomic(path, modified.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;

    log::info!(
        "Renamed key '{}' to '{}' in {}",
        old_key,
        new_key,
        path.display()
    );
    Ok(())
}

/// Rename a key in a JSON value using dot notation.
fn rename_key_in_value(value: &mut serde_json::Value, old_key: &str, new_key: &str) {
    let parts: Vec<&str> = old_key.split('.').collect();
    if parts.is_empty() {
        return;
    }

    if parts.len() == 1 {
        // Direct rename
        if let serde_json::Value::Object(map) = value {
            if let Some(v) = map.remove(parts[0]) {
                map.insert(new_key.to_string(), v);
            }
        }
    } else {
        // Navigate to parent and rename
        let parent_key = parts[..parts.len() - 1].join(".");
        let child_key = parts[parts.len() - 1];

        // Get just the last part of new_key for the insertion
        let new_key_name = new_key.split('.').next_back().unwrap_or(new_key);

        if let Some(serde_json::Value::Object(map)) = get_nested_value_mut(value, &parent_key) {
            if let Some(v) = map.remove(child_key) {
                map.insert(new_key_name.to_string(), v);
            }
        }
    }
}

/// Prompt user with Y/n question, returns true if yes.
fn prompt_yes_no(message: &str, default_yes: bool) -> Result<bool> {
    let prompt = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{} {}: ", message, prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        Ok(default_yes)
    } else {
        Ok(trimmed == "y" || trimmed == "yes")
    }
}

/// Check if stdin is a TTY (interactive terminal).
fn is_tty() -> bool {
    atty::is(atty::Stream::Stdin)
}

/// Check if sanity checks should run for a given command.
///
/// This is used in main.rs to determine whether to run sanity checks
/// before executing a command.
pub fn should_run_sanity_checks(command: &crate::cli::Command) -> bool {
    use crate::cli;

    match command {
        // Commands that should trigger sanity checks
        cli::Command::Run(_) => true,
        cli::Command::Queue(args) => {
            // Only for validate subcommand
            matches!(args.command, cli::queue::QueueCommand::Validate)
        }
        // Doctor runs its own sanity checks with its own flags
        cli::Command::Doctor(_) => false,
        // Other commands don't need sanity checks
        _ => false,
    }
}

/// Report sanity check results to the user.
///
/// Returns true if the operation should proceed, false if it should be aborted.
pub fn report_sanity_results(result: &SanityResult, auto_fix: bool) -> bool {
    // If there are issues that need attention and we're not in auto-fix mode,
    // we might want to abort or warn the user
    if !result.needs_attention.is_empty() && !auto_fix {
        let has_errors = result
            .needs_attention
            .iter()
            .any(|i| i.severity == IssueSeverity::Error);

        if has_errors {
            log::error!("Sanity checks found errors that need to be resolved.");
            log::info!(
                "Run with --auto-fix to automatically fix issues, or resolve them manually."
            );
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn get_known_config_keys_includes_top_level() {
        let keys = get_known_config_keys();
        assert!(keys.contains("version"));
        assert!(keys.contains("agent"));
        assert!(keys.contains("queue"));
        assert!(keys.contains("tui"));
    }

    #[test]
    fn get_known_config_keys_includes_agent_keys() {
        let keys = get_known_config_keys();
        assert!(keys.contains("agent.runner"));
        assert!(keys.contains("agent.model"));
        assert!(keys.contains("agent.phases"));
        assert!(keys.contains("agent.codex_bin"));
        // Extended agent keys that were previously missing
        assert!(keys.contains("agent.update_task_before_run"));
        assert!(keys.contains("agent.fail_on_prerun_update_error"));
        assert!(keys.contains("agent.followup_reasoning_effort"));
        assert!(keys.contains("agent.claude_permission_mode"));
        assert!(keys.contains("agent.runner_cli"));
        // Notification keys
        assert!(keys.contains("agent.notification"));
        assert!(keys.contains("agent.notification.enabled"));
        assert!(keys.contains("agent.notification.notify_on_complete"));
    }

    #[test]
    fn get_known_config_keys_extracts_runner_cli_keys() {
        let keys = get_known_config_keys();
        // runner_cli is a nested config with its own definition
        assert!(keys.contains("agent.runner_cli"));
        assert!(keys.contains("agent.runner_cli.defaults"));
        assert!(keys.contains("agent.runner_cli.runners"));
    }

    #[test]
    fn check_config_file_unknown_keys_detects_unknown() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        std::fs::write(
            &config_path,
            r#"{
                "version": 1,
                "unknown_key": "value",
                "agent": {
                    "runner": "claude",
                    "unknown_agent_key": 123
                }
            }"#,
        )
        .unwrap();

        let known_keys = get_known_config_keys();
        let unknown = check_config_file_unknown_keys(&config_path, &known_keys).unwrap();

        assert!(unknown.contains(&"unknown_key".to_string()));
        assert!(unknown.contains(&"agent.unknown_agent_key".to_string()));
        assert!(!unknown.contains(&"version".to_string()));
        assert!(!unknown.contains(&"agent.runner".to_string()));
    }

    #[test]
    fn remove_key_from_value_works() {
        let mut value: serde_json::Value = serde_json::json!({
            "version": 1,
            "agent": {
                "runner": "claude",
                "model": "sonnet"
            }
        });

        remove_key_from_value(&mut value, "version");
        assert!(value.get("version").is_none());
        assert!(value.get("agent").is_some());

        remove_key_from_value(&mut value, "agent.runner");
        let agent = value.get("agent").unwrap();
        assert!(agent.get("runner").is_none());
        assert!(agent.get("model").is_some());
    }

    #[test]
    fn rename_key_in_value_works() {
        let mut value: serde_json::Value = serde_json::json!({
            "version": 1,
            "agent": {
                "runner": "claude"
            }
        });

        rename_key_in_value(&mut value, "agent.runner", "agent.runner_cli");
        let agent = value.get("agent").unwrap();
        assert!(agent.get("runner").is_none());
        assert_eq!(agent.get("runner_cli").unwrap(), "claude");
    }

    #[test]
    fn is_known_parent_key_detects_parents() {
        let keys = get_known_config_keys();
        assert!(is_known_parent_key("agent", &keys));
        assert!(is_known_parent_key("queue", &keys));
        assert!(!is_known_parent_key("unknown", &keys));
    }
}
