//! Purpose: Provide the public migration API surface and top-level
//! orchestration.
//!
//! Responsibilities:
//! - Declare migration companion modules.
//! - Re-export migration data models and context types.
//! - Check, apply, and list migrations without owning leaf migration logic.
//!
//! Scope:
//! - Top-level orchestration only; type definitions live in `types.rs`, history
//!   persistence lives in `history.rs`, and leaf migration behavior lives in
//!   `config_migrations/` and `file_migrations/`.
//!
//! Usage:
//! - Import migration helpers through `crate::migration`.
//! - Internal modules may use the re-exported types or sibling companion
//!   modules.
//!
//! Invariants/Assumptions:
//! - Re-exports preserve existing caller imports.
//! - Migrations remain idempotent and registry-driven.
//! - Migration history is saved immediately after each successful apply.

use anyhow::{Context, Result};

pub mod config_migrations;
pub mod file_migrations;
pub mod history;
pub mod registry;
mod types;

#[cfg(test)]
mod tests;

pub use types::{
    Migration, MigrationCheckResult, MigrationContext, MigrationStatus, MigrationType,
};

/// Check for pending migrations without applying them.
pub fn check_migrations(ctx: &MigrationContext) -> Result<MigrationCheckResult> {
    let pending: Vec<&'static Migration> = registry::MIGRATIONS
        .iter()
        .filter(|migration| {
            !ctx.is_migration_applied(migration.id) && is_migration_applicable(ctx, migration)
        })
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
            if let Ok(result) =
                crate::commands::init::readme::check_readme_current_from_root(&ctx.repo_root)
            {
                match result {
                    crate::commands::init::readme::ReadmeCheckResult::Current(version) => {
                        version < *from_version
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

    ctx.migration_history
        .applied_migrations
        .push(history::AppliedMigration {
            id: migration.id.to_string(),
            applied_at: chrono::Utc::now(),
            migration_type: format!("{:?}", migration.migration_type),
        });

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

    let (status, _) = crate::commands::init::readme::write_readme(&readme_path, false, true)
        .context("write updated README")?;

    match status {
        crate::commands::init::FileInitStatus::Updated => Ok(()),
        crate::commands::init::FileInitStatus::Created => Ok(()),
        crate::commands::init::FileInitStatus::Valid => Ok(()),
    }
}

/// List all migrations with their status.
pub fn list_migrations(ctx: &MigrationContext) -> Vec<MigrationStatus<'_>> {
    registry::MIGRATIONS
        .iter()
        .map(|migration| {
            let applied = ctx.is_migration_applied(migration.id);
            let applicable = is_migration_applicable(ctx, migration);
            MigrationStatus {
                migration,
                applied,
                applicable,
            }
        })
        .collect()
}
