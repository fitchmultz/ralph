//! File migration utilities for renaming/moving files.
//!
//! Responsibilities:
//! - Safely rename/move files with backup and rollback capability.
//! - Update config references when files are moved.
//! - Handle queue.json to queue.jsonc migration.
//!
//! Not handled here:
//! - Config key renames (see `config_migrations.rs`).
//! - Migration history tracking (see `history.rs`).
//!
//! Invariants/assumptions:
//! - Generic file rename helpers can keep backups before moving.
//! - Config file references are updated after file moves.
//! - JSON-to-JSONC migrations remove legacy `.json` files after `.jsonc` is established.

use crate::config;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::MigrationContext;

/// Options for file migration.
#[derive(Debug, Clone)]
pub struct FileMigrationOptions {
    /// Whether to keep the original file as a backup.
    pub keep_backup: bool,
    /// Whether to update config file references.
    pub update_config: bool,
}

impl Default for FileMigrationOptions {
    fn default() -> Self {
        Self {
            keep_backup: true,
            update_config: true,
        }
    }
}

/// Apply a file rename migration.
/// Copies content from old_path to new_path and optionally updates config.
pub fn apply_file_rename(ctx: &MigrationContext, old_path: &str, new_path: &str) -> Result<()> {
    let opts = FileMigrationOptions::default();
    apply_file_rename_with_options(ctx, old_path, new_path, &opts)
}

/// Apply a file rename migration with custom options.
pub fn apply_file_rename_with_options(
    ctx: &MigrationContext,
    old_path: &str,
    new_path: &str,
    opts: &FileMigrationOptions,
) -> Result<()> {
    let old_full_path = ctx.resolve_path(old_path);
    let new_full_path = ctx.resolve_path(new_path);

    // Validate source exists
    if !old_full_path.exists() {
        anyhow::bail!("Source file does not exist: {}", old_full_path.display());
    }

    // Validate destination doesn't exist (unless it's the same file)
    if new_full_path.exists() && old_full_path != new_full_path {
        anyhow::bail!(
            "Destination file already exists: {}",
            new_full_path.display()
        );
    }

    // Ensure parent directory exists for destination
    if let Some(parent) = new_full_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent directory {}", parent.display()))?;
    }

    // Copy the file (preserves content, creates new file)
    fs::copy(&old_full_path, &new_full_path).with_context(|| {
        format!(
            "copy {} to {}",
            old_full_path.display(),
            new_full_path.display()
        )
    })?;

    log::info!(
        "Migrated file from {} to {}",
        old_full_path.display(),
        new_full_path.display()
    );

    // Update config references if needed
    if opts.update_config {
        update_config_file_references(ctx, old_path, new_path)
            .context("update config file references")?;
    }

    // Remove original if not keeping backup
    if !opts.keep_backup {
        fs::remove_file(&old_full_path)
            .with_context(|| format!("remove original file {}", old_full_path.display()))?;
        log::debug!("Removed original file {}", old_full_path.display());
    } else {
        log::debug!("Kept original file {} as backup", old_full_path.display());
    }

    Ok(())
}

/// Update config file references after a file move.
/// Updates queue.file and queue.done_file if they match the old path.
fn update_config_file_references(
    ctx: &MigrationContext,
    old_path: &str,
    new_path: &str,
) -> Result<()> {
    // Check project config
    if ctx.project_config_path.exists() {
        update_config_file_if_needed(&ctx.project_config_path, old_path, new_path)
            .context("update project config file references")?;
    }

    // Check global config
    if let Some(global_path) = &ctx.global_config_path
        && global_path.exists()
    {
        update_config_file_if_needed(global_path, old_path, new_path)
            .context("update global config file references")?;
    }

    Ok(())
}

/// Update a specific config file's file references.
fn update_config_file_if_needed(config_path: &Path, old_path: &str, new_path: &str) -> Result<()> {
    // Load the config layer
    let layer = config::load_layer(config_path)
        .with_context(|| format!("load config from {}", config_path.display()))?;

    let old_path_buf = PathBuf::from(old_path);
    let _new_path_buf = PathBuf::from(new_path);

    // Check if any file references need updating
    let mut needs_update = false;

    if let Some(ref file) = layer.queue.file
        && file == &old_path_buf
    {
        needs_update = true;
    }

    if let Some(ref done_file) = layer.queue.done_file
        && done_file == &old_path_buf
    {
        needs_update = true;
    }

    if !needs_update {
        return Ok(());
    }

    // We need to do a text-based replacement to preserve JSONC comments
    let raw = fs::read_to_string(config_path)
        .with_context(|| format!("read config {}", config_path.display()))?;

    // Replace the old path with the new path
    // We use a simple string replacement for the specific path value
    let updated = raw.replace(&format!("\"{}\"", old_path), &format!("\"{}\"", new_path));

    // Write back
    crate::fsutil::write_atomic(config_path, updated.as_bytes())
        .with_context(|| format!("write updated config to {}", config_path.display()))?;

    log::info!(
        "Updated file reference in {}: {} -> {}",
        config_path.display(),
        old_path,
        new_path
    );

    Ok(())
}

/// Migrate queue.json to queue.jsonc.
/// This is a convenience function for the common case.
pub fn migrate_queue_json_to_jsonc(ctx: &MigrationContext) -> Result<()> {
    migrate_json_to_jsonc(ctx, ".ralph/queue.json", ".ralph/queue.jsonc")
        .context("migrate queue.json to queue.jsonc")
}

/// Migrate done.json to done.jsonc.
pub fn migrate_done_json_to_jsonc(ctx: &MigrationContext) -> Result<()> {
    migrate_json_to_jsonc(ctx, ".ralph/done.json", ".ralph/done.jsonc")
        .context("migrate done.json to done.jsonc")
}

/// Check if a migration from queue.json to queue.jsonc is applicable.
pub fn is_queue_json_to_jsonc_applicable(ctx: &MigrationContext) -> bool {
    ctx.file_exists(".ralph/queue.json")
}

/// Check if a migration from done.json to done.jsonc is applicable.
pub fn is_done_json_to_jsonc_applicable(ctx: &MigrationContext) -> bool {
    ctx.file_exists(".ralph/done.json")
}

/// Migrate config.json to config.jsonc.
pub fn migrate_config_json_to_jsonc(ctx: &MigrationContext) -> Result<()> {
    migrate_json_to_jsonc(ctx, ".ralph/config.json", ".ralph/config.jsonc")
        .context("migrate config.json to config.jsonc")
}

/// Check if a migration from config.json to config.jsonc is applicable.
pub fn is_config_json_to_jsonc_applicable(ctx: &MigrationContext) -> bool {
    ctx.file_exists(".ralph/config.json")
}

fn migrate_json_to_jsonc(ctx: &MigrationContext, old_path: &str, new_path: &str) -> Result<()> {
    let old_full_path = ctx.resolve_path(old_path);
    let new_full_path = ctx.resolve_path(new_path);

    if !old_full_path.exists() {
        return Ok(());
    }

    if new_full_path.exists() {
        // Queue/done/config references may still point at legacy .json paths even when
        // .jsonc already exists. Normalize references before deleting legacy files.
        update_config_file_references(ctx, old_path, new_path)
            .context("update config references for established jsonc migration")?;
        fs::remove_file(&old_full_path)
            .with_context(|| format!("remove legacy file {}", old_full_path.display()))?;
        log::info!(
            "Removed legacy file {} because {} already exists",
            old_full_path.display(),
            new_full_path.display()
        );
        return Ok(());
    }

    let opts = FileMigrationOptions {
        keep_backup: false,
        update_config: true,
    };
    apply_file_rename_with_options(ctx, old_path, new_path, &opts)
}

/// Rollback a file migration by restoring from backup.
/// This removes the new file and restores the original.
pub fn rollback_file_migration(
    ctx: &MigrationContext,
    old_path: &str,
    new_path: &str,
) -> Result<()> {
    let old_full_path = ctx.resolve_path(old_path);
    let new_full_path = ctx.resolve_path(new_path);

    // Check that original exists (backup)
    if !old_full_path.exists() {
        anyhow::bail!(
            "Cannot rollback: original file {} does not exist",
            old_full_path.display()
        );
    }

    // Remove the new file
    if new_full_path.exists() {
        fs::remove_file(&new_full_path)
            .with_context(|| format!("remove migrated file {}", new_full_path.display()))?;
    }

    log::info!(
        "Rolled back file migration: restored {}, removed {}",
        old_full_path.display(),
        new_full_path.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_context(dir: &TempDir) -> MigrationContext {
        let repo_root = dir.path().to_path_buf();
        let project_config_path = repo_root.join(".ralph/config.json");

        MigrationContext {
            repo_root,
            project_config_path,
            global_config_path: None,
            resolved_config: crate::contracts::Config::default(),
            migration_history: super::super::history::MigrationHistory::default(),
        }
    }

    #[test]
    fn apply_file_rename_copies_file() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Create source file
        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        let source = dir.path().join(".ralph/queue.json");
        fs::write(&source, "{\"version\": 1}").unwrap();

        // Apply migration
        apply_file_rename(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc").unwrap();

        // Both files should exist (backup kept by default)
        assert!(source.exists());
        assert!(dir.path().join(".ralph/queue.jsonc").exists());

        // Content should be identical
        let original_content = fs::read_to_string(&source).unwrap();
        let new_content = fs::read_to_string(dir.path().join(".ralph/queue.jsonc")).unwrap();
        assert_eq!(original_content, new_content);
    }

    #[test]
    fn apply_file_rename_without_backup_removes_original() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Create source file
        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        let source = dir.path().join(".ralph/queue.json");
        fs::write(&source, "{\"version\": 1}").unwrap();

        // Apply migration without backup
        let opts = FileMigrationOptions {
            keep_backup: false,
            update_config: false,
        };
        apply_file_rename_with_options(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc", &opts)
            .unwrap();

        // Original should be gone, new should exist
        assert!(!source.exists());
        assert!(dir.path().join(".ralph/queue.jsonc").exists());
    }

    #[test]
    fn apply_file_rename_fails_if_source_missing() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        let result = apply_file_rename(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn apply_file_rename_fails_if_destination_exists() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Create both files
        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        fs::write(dir.path().join(".ralph/queue.json"), "{}").unwrap();
        fs::write(dir.path().join(".ralph/queue.jsonc"), "{}").unwrap();

        let result = apply_file_rename(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn update_config_file_references_updates_queue_file() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Create config with queue.file
        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        fs::write(
            &ctx.project_config_path,
            r#"{
                "version": 1,
                "queue": {
                    "file": ".ralph/queue.json"
                }
            }"#,
        )
        .unwrap();

        // Update references
        update_config_file_references(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc").unwrap();

        // Verify update
        let content = fs::read_to_string(&ctx.project_config_path).unwrap();
        assert!(content.contains("\"file\": \".ralph/queue.jsonc\""));
        assert!(!content.contains("\"file\": \".ralph/queue.json\""));
    }

    #[test]
    fn update_config_file_references_updates_done_file() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Create config with queue.done_file
        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        fs::write(
            &ctx.project_config_path,
            r#"{
                "version": 1,
                "queue": {
                    "done_file": ".ralph/done.json"
                }
            }"#,
        )
        .unwrap();

        // Update references
        update_config_file_references(&ctx, ".ralph/done.json", ".ralph/done.jsonc").unwrap();

        // Verify update
        let content = fs::read_to_string(&ctx.project_config_path).unwrap();
        assert!(content.contains("\"done_file\": \".ralph/done.jsonc\""));
    }

    #[test]
    fn is_queue_json_to_jsonc_applicable_detects_correct_state() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Neither file exists
        assert!(!is_queue_json_to_jsonc_applicable(&ctx));

        // Only queue.json exists
        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        fs::write(dir.path().join(".ralph/queue.json"), "{}").unwrap();
        assert!(is_queue_json_to_jsonc_applicable(&ctx));

        // Both exist
        fs::write(dir.path().join(".ralph/queue.jsonc"), "{}").unwrap();
        assert!(is_queue_json_to_jsonc_applicable(&ctx));
    }

    #[test]
    fn migrate_queue_json_to_jsonc_removes_legacy_file_when_jsonc_absent() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        fs::write(dir.path().join(".ralph/queue.json"), "{\"version\": 1}").unwrap();

        migrate_queue_json_to_jsonc(&ctx).unwrap();

        assert!(!dir.path().join(".ralph/queue.json").exists());
        assert!(dir.path().join(".ralph/queue.jsonc").exists());
    }

    #[test]
    fn migrate_queue_json_to_jsonc_removes_legacy_file_when_jsonc_already_exists() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        fs::write(dir.path().join(".ralph/queue.json"), "{\"legacy\": true}").unwrap();
        fs::write(dir.path().join(".ralph/queue.jsonc"), "{\"version\": 1}").unwrap();

        migrate_queue_json_to_jsonc(&ctx).unwrap();

        assert!(!dir.path().join(".ralph/queue.json").exists());
        assert!(dir.path().join(".ralph/queue.jsonc").exists());
    }

    #[test]
    fn rollback_file_migration_restores_original() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Create source file and migrate
        fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        fs::write(dir.path().join(".ralph/queue.json"), "{\"version\": 1}").unwrap();
        apply_file_rename(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc").unwrap();

        // Verify both exist
        assert!(dir.path().join(".ralph/queue.json").exists());
        assert!(dir.path().join(".ralph/queue.jsonc").exists());

        // Rollback
        rollback_file_migration(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc").unwrap();

        // Original should exist, new should be gone
        assert!(dir.path().join(".ralph/queue.json").exists());
        assert!(!dir.path().join(".ralph/queue.jsonc").exists());
    }

    #[test]
    fn rollback_fails_if_original_missing() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Try to rollback without original
        let result = rollback_file_migration(&ctx, ".ralph/queue.json", ".ralph/queue.jsonc");
        assert!(result.is_err());
    }
}
