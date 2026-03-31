//! Purpose: Own migration data models, status enums, and context construction.
//!
//! Responsibilities:
//! - Define migration model types used by migration orchestration and callers.
//! - Build `MigrationContext` values from resolved config or filesystem discovery.
//! - Provide display-oriented status helpers for migration listings.
//!
//! Scope:
//! - Type definitions and context-building only; migration execution remains in
//!   `mod.rs`.
//!
//! Usage:
//! - Re-exported through `crate::migration::{Migration, MigrationContext, ...}`.
//! - Used by migration orchestration, CLI surfaces, and sanity checks.
//!
//! Invariants/Assumptions:
//! - `MigrationContext` points at a single repo root and config pair.
//! - Context discovery does not require config parsing to succeed.
//! - Migration history is loaded eagerly during context construction.

use super::history;
use crate::config::Resolved;
use anyhow::{Context, Result};
use std::{
    env,
    path::{Path, PathBuf},
};

/// Result of checking migration status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationCheckResult {
    /// No pending migrations.
    Current,
    /// Pending migrations available.
    Pending(Vec<&'static Migration>),
}

/// A single migration definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// Unique identifier for this migration (e.g., "config_key_rename_2026_01").
    pub id: &'static str,
    /// Human-readable description of what this migration does.
    pub description: &'static str,
    /// The type of migration to perform.
    pub migration_type: MigrationType,
}

/// Types of migrations supported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationType {
    /// Rename a config key (old_key -> new_key).
    ConfigKeyRename {
        /// Dot-separated path to the old key (e.g., "agent.runner_cli").
        old_key: &'static str,
        /// Dot-separated path to the new key (e.g., "agent.runner_options").
        new_key: &'static str,
    },
    /// Remove a deprecated config key.
    ConfigKeyRemove {
        /// Dot-separated path to the key to remove (e.g., "agent.legacy_flag").
        key: &'static str,
    },
    /// Rewrite legacy CI gate string config to structured argv/shell config.
    ConfigCiGateRewrite,
    /// Upgrade pre-0.3 config contract keys and version markers.
    ConfigLegacyContractUpgrade,
    /// Rename/move a file.
    FileRename {
        /// Path to the old file, relative to repo root.
        old_path: &'static str,
        /// Path to the new file, relative to repo root.
        new_path: &'static str,
    },
    /// Update README template.
    ReadmeUpdate {
        /// The version to update from (inclusive).
        from_version: u32,
        /// The version to update to.
        to_version: u32,
    },
}

/// Context for migration operations.
#[derive(Debug, Clone)]
pub struct MigrationContext {
    /// Repository root directory.
    pub repo_root: PathBuf,
    /// Path to project config file.
    pub project_config_path: PathBuf,
    /// Path to global config file (if any).
    pub global_config_path: Option<PathBuf>,
    /// Currently resolved configuration.
    pub resolved_config: crate::contracts::Config,
    /// Loaded migration history.
    pub migration_history: history::MigrationHistory,
}

impl MigrationContext {
    /// Create a new migration context from resolved config.
    pub fn from_resolved(resolved: &Resolved) -> Result<Self> {
        Self::build(
            resolved.repo_root.clone(),
            resolved
                .project_config_path
                .clone()
                .unwrap_or_else(|| resolved.repo_root.join(".ralph/config.jsonc")),
            resolved.global_config_path.clone(),
            resolved.config.clone(),
        )
    }

    /// Create a migration context from the current working directory without
    /// requiring configuration parsing to succeed.
    pub fn discover_from_cwd() -> Result<Self> {
        let cwd = env::current_dir().context("resolve current working directory")?;
        Self::discover_from_dir(&cwd)
    }

    /// Create a migration context from an arbitrary directory without
    /// requiring configuration parsing to succeed.
    pub fn discover_from_dir(start: &Path) -> Result<Self> {
        let repo_root = crate::config::find_repo_root(start);
        let project_config_path = crate::config::project_config_path(&repo_root);
        let global_config_path = crate::config::global_config_path();

        Self::build(
            repo_root,
            project_config_path,
            global_config_path,
            crate::contracts::Config::default(),
        )
    }

    fn build(
        repo_root: PathBuf,
        project_config_path: PathBuf,
        global_config_path: Option<PathBuf>,
        resolved_config: crate::contracts::Config,
    ) -> Result<Self> {
        let migration_history =
            history::load_migration_history(&repo_root).context("load migration history")?;

        Ok(Self {
            repo_root,
            project_config_path,
            global_config_path,
            resolved_config,
            migration_history,
        })
    }

    /// Check if a migration has already been applied.
    pub fn is_migration_applied(&self, migration_id: &str) -> bool {
        self.migration_history
            .applied_migrations
            .iter()
            .any(|migration| migration.id == migration_id)
    }

    /// Check if a file exists relative to repo root.
    pub fn file_exists(&self, path: &str) -> bool {
        self.repo_root.join(path).exists()
    }

    /// Get full path for a repo-relative path.
    pub fn resolve_path(&self, path: &str) -> PathBuf {
        self.repo_root.join(path)
    }
}

/// Status of a migration for display.
#[derive(Debug, Clone)]
pub struct MigrationStatus<'a> {
    /// The migration definition.
    pub migration: &'a Migration,
    /// Whether this migration has been applied.
    pub applied: bool,
    /// Whether this migration is applicable in the current context.
    pub applicable: bool,
}

impl<'a> MigrationStatus<'a> {
    /// Get a display status string.
    pub fn status_text(&self) -> &'static str {
        if self.applied {
            "applied"
        } else if self.applicable {
            "pending"
        } else {
            "not applicable"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::history;
    use tempfile::TempDir;

    fn create_test_context(dir: &TempDir) -> MigrationContext {
        let repo_root = dir.path().to_path_buf();
        let project_config_path = repo_root.join(".ralph/config.jsonc");

        MigrationContext {
            repo_root,
            project_config_path,
            global_config_path: None,
            resolved_config: crate::contracts::Config::default(),
            migration_history: history::MigrationHistory::default(),
        }
    }

    #[test]
    fn migration_context_detects_applied_migration() {
        let dir = TempDir::new().unwrap();
        let mut ctx = create_test_context(&dir);

        assert!(!ctx.is_migration_applied("test_migration"));

        ctx.migration_history
            .applied_migrations
            .push(history::AppliedMigration {
                id: "test_migration".to_string(),
                applied_at: chrono::Utc::now(),
                migration_type: "test".to_string(),
            });

        assert!(ctx.is_migration_applied("test_migration"));
    }

    #[test]
    fn migration_context_file_exists_check() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        std::fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        std::fs::write(dir.path().join(".ralph/queue.json"), "{}").unwrap();

        assert!(ctx.file_exists(".ralph/queue.json"));
        assert!(!ctx.file_exists(".ralph/done.json"));
    }

    #[test]
    fn migration_context_discovers_repo_without_resolving_config() {
        let dir = TempDir::new().unwrap();
        let ralph_dir = dir.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();
        std::fs::write(
            ralph_dir.join("config.jsonc"),
            r#"{"version":1,"agent":{"git_commit_push_enabled":true}}"#,
        )
        .unwrap();

        let ctx = MigrationContext::discover_from_dir(dir.path()).unwrap();

        assert_eq!(ctx.repo_root, dir.path());
        assert_eq!(ctx.project_config_path, ralph_dir.join("config.jsonc"));
    }

    #[test]
    fn migration_status_reports_display_text() {
        let migration = Migration {
            id: "test",
            description: "test migration",
            migration_type: MigrationType::ConfigCiGateRewrite,
        };

        assert_eq!(
            MigrationStatus {
                migration: &migration,
                applied: true,
                applicable: true,
            }
            .status_text(),
            "applied"
        );
        assert_eq!(
            MigrationStatus {
                migration: &migration,
                applied: false,
                applicable: true,
            }
            .status_text(),
            "pending"
        );
        assert_eq!(
            MigrationStatus {
                migration: &migration,
                applied: false,
                applicable: false,
            }
            .status_text(),
            "not applicable"
        );
    }
}
