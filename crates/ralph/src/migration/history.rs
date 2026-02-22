//! Migration history persistence for tracking applied migrations.
//!
//! Responsibilities:
//! - Load and save migration history from `.ralph/cache/migrations.jsonc`.
//! - Provide default history for new projects.
//!
//! Not handled here:
//! - Migration execution logic (see `super::mod.rs`).
//! - Config file modifications (see `config_migrations.rs`).
//!
//! Invariants/assumptions:
//! - History file is stored in `.ralph/cache/migrations.jsonc`.
//! - History format is versioned for future compatibility.

use crate::constants::paths::MIGRATION_HISTORY_PATH;
use crate::constants::versions::HISTORY_VERSION;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Migration history tracking all applied migrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MigrationHistory {
    /// Schema version for migration history.
    pub version: u32,
    /// List of applied migrations.
    pub applied_migrations: Vec<AppliedMigration>,
}

impl Default for MigrationHistory {
    fn default() -> Self {
        Self {
            version: HISTORY_VERSION,
            applied_migrations: Vec::new(),
        }
    }
}

/// A single applied migration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedMigration {
    /// Unique identifier for the migration.
    pub id: String,
    /// Timestamp when the migration was applied.
    pub applied_at: DateTime<Utc>,
    /// Type of migration (for informational purposes).
    pub migration_type: String,
}

/// Load migration history from the repo.
/// Returns default (empty) history if file doesn't exist.
pub fn load_migration_history(repo_root: &Path) -> Result<MigrationHistory> {
    let history_path = repo_root.join(MIGRATION_HISTORY_PATH);

    if !history_path.exists() {
        log::debug!(
            "Migration history not found at {}, using default",
            history_path.display()
        );
        return Ok(MigrationHistory::default());
    }

    let raw = fs::read_to_string(&history_path)
        .with_context(|| format!("read migration history from {}", history_path.display()))?;

    let history: MigrationHistory = serde_json::from_str(&raw)
        .with_context(|| format!("parse migration history from {}", history_path.display()))?;

    // Validate version
    if history.version != HISTORY_VERSION {
        log::warn!(
            "Migration history version mismatch: expected {}, got {}. Attempting to proceed.",
            HISTORY_VERSION,
            history.version
        );
    }

    log::debug!(
        "Loaded migration history with {} applied migrations",
        history.applied_migrations.len()
    );

    Ok(history)
}

/// Save migration history to the repo.
pub fn save_migration_history(repo_root: &Path, history: &MigrationHistory) -> Result<()> {
    let history_path = repo_root.join(MIGRATION_HISTORY_PATH);

    // Ensure parent directory exists
    if let Some(parent) = history_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create migration history directory {}", parent.display()))?;
    }

    let raw =
        serde_json::to_string_pretty(history).context("serialize migration history to JSON")?;

    crate::fsutil::write_atomic(&history_path, raw.as_bytes())
        .with_context(|| format!("write migration history to {}", history_path.display()))?;

    log::debug!(
        "Saved migration history with {} applied migrations",
        history.applied_migrations.len()
    );

    Ok(())
}

/// Get the path to the migration history file.
pub fn migration_history_path(repo_root: &Path) -> PathBuf {
    repo_root.join(MIGRATION_HISTORY_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_migration_history_returns_default_when_missing() {
        let dir = TempDir::new().unwrap();
        let history = load_migration_history(dir.path()).unwrap();

        assert_eq!(history.version, HISTORY_VERSION);
        assert!(history.applied_migrations.is_empty());
    }

    #[test]
    fn save_and_load_migration_history_round_trips() {
        let dir = TempDir::new().unwrap();

        // Create and save a history
        let mut history = MigrationHistory::default();
        history.applied_migrations.push(AppliedMigration {
            id: "test_migration_1".to_string(),
            applied_at: Utc::now(),
            migration_type: "config_key_rename".to_string(),
        });
        history.applied_migrations.push(AppliedMigration {
            id: "test_migration_2".to_string(),
            applied_at: Utc::now(),
            migration_type: "file_rename".to_string(),
        });

        save_migration_history(dir.path(), &history).unwrap();

        // Load it back
        let loaded = load_migration_history(dir.path()).unwrap();

        assert_eq!(loaded.version, HISTORY_VERSION);
        assert_eq!(loaded.applied_migrations.len(), 2);
        assert_eq!(loaded.applied_migrations[0].id, "test_migration_1");
        assert_eq!(loaded.applied_migrations[1].id, "test_migration_2");
    }

    #[test]
    fn migration_history_path_is_correct() {
        let dir = PathBuf::from("/tmp/test_repo");
        let path = migration_history_path(&dir);

        assert_eq!(
            path,
            PathBuf::from("/tmp/test_repo/.ralph/cache/migrations.jsonc")
        );
    }

    #[test]
    fn save_migration_history_creates_parent_directories() {
        let dir = TempDir::new().unwrap();
        let deep_path = dir.path().join(".ralph/cache");

        // Ensure the directory doesn't exist yet
        assert!(!deep_path.exists());

        let history = MigrationHistory::default();
        save_migration_history(dir.path(), &history).unwrap();

        // Directory should now exist
        assert!(deep_path.exists());
    }
}
