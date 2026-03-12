//! Migration system for config and project file changes.
//!
//! Responsibilities:
//! - Track and apply migrations for config key renames/removals, file format changes, and README updates.
//! - Provide a registry-based system for defining migrations with unique IDs.
//! - Support safe migration with backup/rollback capability.
//! - Preserve JSONC comments when modifying config files.
//!
//! Not handled here:
//! - Direct file I/O for migration history (see `history.rs`).
//! - Config key rename implementation details (see `config_migrations.rs`).
//! - File migration implementation details (see `file_migrations.rs`).
//!
//! Invariants/assumptions:
//! - Migrations are idempotent: running the same migration twice is a no-op.
//! - Migration history is stored in `.ralph/cache/migrations.jsonc`.
//! - All migrations have a unique ID and are tracked in the registry.

use crate::config::Resolved;
use anyhow::{Context, Result};
use std::{env, path::PathBuf};

pub mod config_migrations;
pub mod file_migrations;
pub mod history;
pub mod registry;

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
    pub fn discover_from_dir(start: &std::path::Path) -> Result<Self> {
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
            .any(|m| m.id == migration_id)
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

/// Check for pending migrations without applying them.
pub fn check_migrations(ctx: &MigrationContext) -> Result<MigrationCheckResult> {
    let pending: Vec<&'static Migration> = registry::MIGRATIONS
        .iter()
        .filter(|m| !ctx.is_migration_applied(m.id) && is_migration_applicable(ctx, m))
        .collect();

    if pending.is_empty() {
        Ok(MigrationCheckResult::Current)
    } else {
        Ok(MigrationCheckResult::Pending(pending))
    }
}

/// Check if a specific migration is applicable in the current context.
fn is_migration_applicable(ctx: &MigrationContext, migration: &Migration) -> bool {
    match &migration.migration_type {
        MigrationType::ConfigKeyRename { old_key, .. } => {
            config_migrations::config_has_key(ctx, old_key)
        }
        MigrationType::ConfigKeyRemove { key } => config_migrations::config_has_key(ctx, key),
        MigrationType::ConfigCiGateRewrite => {
            config_migrations::config_has_key(ctx, "agent.ci_gate_command")
                || config_migrations::config_has_key(ctx, "agent.ci_gate_enabled")
        }
        MigrationType::ConfigLegacyContractUpgrade => {
            config_migrations::config_needs_legacy_contract_upgrade(ctx)
        }
        MigrationType::FileRename { old_path, new_path } => {
            if matches!(
                migration.id,
                "file_cleanup_legacy_queue_json_after_jsonc_2026_02"
                    | "file_cleanup_legacy_done_json_after_jsonc_2026_02"
                    | "file_cleanup_legacy_config_json_after_jsonc_2026_02"
            ) {
                return ctx.file_exists(old_path) && ctx.file_exists(new_path);
            }
            match (*old_path, *new_path) {
                (".ralph/queue.json", ".ralph/queue.jsonc")
                | (".ralph/done.json", ".ralph/done.jsonc")
                | (".ralph/config.json", ".ralph/config.jsonc") => ctx.file_exists(old_path),
                _ => ctx.file_exists(old_path) && !ctx.file_exists(new_path),
            }
        }
        MigrationType::ReadmeUpdate { from_version, .. } => {
            // README update is applicable if current version is less than target
            // This is handled separately by the README module
            if let Ok(result) =
                crate::commands::init::readme::check_readme_current_from_root(&ctx.repo_root)
            {
                match result {
                    crate::commands::init::readme::ReadmeCheckResult::Current(v) => {
                        v < *from_version
                    }
                    crate::commands::init::readme::ReadmeCheckResult::Outdated {
                        current_version,
                        ..
                    } => current_version < *from_version,
                    _ => false,
                }
            } else {
                false
            }
        }
    }
}

/// Apply a single migration.
pub fn apply_migration(ctx: &mut MigrationContext, migration: &Migration) -> Result<()> {
    if ctx.is_migration_applied(migration.id) {
        log::debug!("Migration {} already applied, skipping", migration.id);
        return Ok(());
    }

    log::info!(
        "Applying migration: {} - {}",
        migration.id,
        migration.description
    );

    match &migration.migration_type {
        MigrationType::ConfigKeyRename { old_key, new_key } => {
            config_migrations::apply_key_rename(ctx, old_key, new_key)
                .with_context(|| format!("apply config key rename for {}", migration.id))?;
        }
        MigrationType::ConfigKeyRemove { key } => {
            config_migrations::apply_key_remove(ctx, key)
                .with_context(|| format!("apply config key removal for {}", migration.id))?;
        }
        MigrationType::ConfigCiGateRewrite => {
            config_migrations::apply_ci_gate_rewrite(ctx)
                .with_context(|| format!("apply CI gate rewrite for {}", migration.id))?;
        }
        MigrationType::ConfigLegacyContractUpgrade => {
            config_migrations::apply_legacy_contract_upgrade(ctx)
                .with_context(|| format!("apply legacy config upgrade for {}", migration.id))?;
        }
        MigrationType::FileRename { old_path, new_path } => match (*old_path, *new_path) {
            (".ralph/queue.json", ".ralph/queue.jsonc") => {
                file_migrations::migrate_queue_json_to_jsonc(ctx)
                    .with_context(|| format!("apply file rename for {}", migration.id))?;
            }
            (".ralph/done.json", ".ralph/done.jsonc") => {
                file_migrations::migrate_done_json_to_jsonc(ctx)
                    .with_context(|| format!("apply file rename for {}", migration.id))?;
            }
            (".ralph/config.json", ".ralph/config.jsonc") => {
                file_migrations::migrate_config_json_to_jsonc(ctx)
                    .with_context(|| format!("apply file rename for {}", migration.id))?;
            }
            _ => {
                file_migrations::apply_file_rename(ctx, old_path, new_path)
                    .with_context(|| format!("apply file rename for {}", migration.id))?;
            }
        },
        MigrationType::ReadmeUpdate { .. } => {
            apply_readme_update(ctx)
                .with_context(|| format!("apply README update for {}", migration.id))?;
        }
    }

    // Record the migration as applied
    ctx.migration_history
        .applied_migrations
        .push(history::AppliedMigration {
            id: migration.id.to_string(),
            applied_at: chrono::Utc::now(),
            migration_type: format!("{:?}", migration.migration_type),
        });

    // Save the updated history
    history::save_migration_history(&ctx.repo_root, &ctx.migration_history)
        .with_context(|| format!("save migration history after {}", migration.id))?;

    log::info!("Successfully applied migration: {}", migration.id);
    Ok(())
}

/// Apply all pending migrations.
pub fn apply_all_migrations(ctx: &mut MigrationContext) -> Result<Vec<&'static str>> {
    let pending = match check_migrations(ctx)? {
        MigrationCheckResult::Current => return Ok(Vec::new()),
        MigrationCheckResult::Pending(migrations) => migrations,
    };

    let mut applied = Vec::new();
    for migration in pending {
        apply_migration(ctx, migration)
            .with_context(|| format!("apply migration {}", migration.id))?;
        applied.push(migration.id);
    }

    Ok(applied)
}

/// Apply README update migration.
fn apply_readme_update(ctx: &MigrationContext) -> Result<()> {
    let readme_path = ctx.repo_root.join(".ralph/README.md");
    if !readme_path.exists() {
        anyhow::bail!("README.md does not exist at {}", readme_path.display());
    }

    // Use the existing README write functionality
    let (status, _) = crate::commands::init::readme::write_readme(&readme_path, false, true)
        .context("write updated README")?;

    match status {
        crate::commands::init::FileInitStatus::Updated => Ok(()),
        crate::commands::init::FileInitStatus::Created => {
            // This shouldn't happen since we're updating
            Ok(())
        }
        crate::commands::init::FileInitStatus::Valid => {
            // Already current
            Ok(())
        }
    }
}

/// List all migrations with their status.
pub fn list_migrations(ctx: &MigrationContext) -> Vec<MigrationStatus<'_>> {
    registry::MIGRATIONS
        .iter()
        .map(|m| {
            let applied = ctx.is_migration_applied(m.id);
            let applicable = is_migration_applicable(ctx, m);
            MigrationStatus {
                migration: m,
                applied,
                applicable,
            }
        })
        .collect()
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
    use tempfile::TempDir;

    fn create_test_context(dir: &TempDir) -> MigrationContext {
        let repo_root = dir.path().to_path_buf();
        let project_config_path = repo_root.join(".ralph/config.json");

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

        // Initially no migrations applied
        assert!(!ctx.is_migration_applied("test_migration"));

        // Add a migration to history
        ctx.migration_history
            .applied_migrations
            .push(history::AppliedMigration {
                id: "test_migration".to_string(),
                applied_at: chrono::Utc::now(),
                migration_type: "test".to_string(),
            });

        // Now it should be detected as applied
        assert!(ctx.is_migration_applied("test_migration"));
    }

    #[test]
    fn migration_context_file_exists_check() {
        let dir = TempDir::new().unwrap();
        let ctx = create_test_context(&dir);

        // Create a test file
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
    fn cleanup_migration_pending_when_legacy_json_remains_after_rename_migration() {
        let dir = TempDir::new().unwrap();
        let mut ctx = create_test_context(&dir);

        std::fs::create_dir_all(dir.path().join(".ralph")).unwrap();
        std::fs::write(dir.path().join(".ralph/queue.json"), "{}").unwrap();
        std::fs::write(dir.path().join(".ralph/queue.jsonc"), "{}").unwrap();

        // Simulate historical state where rename migration was already recorded.
        ctx.migration_history
            .applied_migrations
            .push(history::AppliedMigration {
                id: "file_rename_queue_json_to_jsonc_2026_02".to_string(),
                applied_at: chrono::Utc::now(),
                migration_type: "FileRename".to_string(),
            });

        let pending = match check_migrations(&ctx).expect("check migrations") {
            MigrationCheckResult::Pending(pending) => pending,
            MigrationCheckResult::Current => panic!("expected pending cleanup migration"),
        };

        let pending_ids: Vec<&str> = pending.iter().map(|m| m.id).collect();
        assert!(
            pending_ids.contains(&"file_cleanup_legacy_queue_json_after_jsonc_2026_02"),
            "expected cleanup migration to be pending when legacy queue.json remains"
        );
    }
}
