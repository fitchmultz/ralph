//! Config migration handling for sanity checks.
//!
//! Purpose:
//! - Config migration handling for sanity checks.
//!
//! Responsibilities:
//! - Check for pending config migrations
//! - Prompt user or auto-apply migrations based on options
//! - Track applied migrations
//!
//! Not handled here:
//! - README updates (see readme.rs)
//! - Unknown key detection (see unknown_keys.rs)
//! - Migration definitions (see migration/ module)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Migrations require user confirmation unless --auto-fix is set
//! - In non-interactive mode without auto-fix, migrations are skipped with warning

use crate::migration::MigrationContext;
use anyhow::{Context, Result};

/// Check for pending config migrations and prompt/apply them.
///
/// Returns a list of migration descriptions that were applied.
pub(crate) fn check_and_handle_migrations(
    ctx: &mut MigrationContext,
    auto_fix: bool,
    non_interactive: bool,
    can_prompt: impl Fn() -> bool,
    prompt_yes_no: impl Fn(&str, bool) -> Result<bool>,
) -> Result<Vec<String>> {
    use crate::migration::{
        MigrationCheckResult, MigrationType, apply_migration, check_migrations,
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
                    MigrationType::ConfigKeyRemove { key } => {
                        format!("Config uses removed key '{}', delete it", key)
                    }
                    MigrationType::ConfigCiGateRewrite => {
                        "Config uses removed CI gate string keys, rewrite to structured agent.ci_gate".to_string()
                    }
                    MigrationType::ConfigLegacyContractUpgrade => {
                        "Config uses the pre-0.3 contract, upgrade to version 2 and agent.git_publish_mode".to_string()
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
                } else if !non_interactive && can_prompt() {
                    prompt_yes_no(&description, true)?
                } else {
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
