//! Migration CLI command for checking and applying config/file migrations.
//!
//! Responsibilities:
//! - Provide CLI interface for migration operations (check, list, apply).
//! - Display migration status to users in a readable format.
//! - Handle user confirmation for destructive operations.
//!
//! Not handled here:
//! - Migration implementation logic (see `crate::migration`).
//! - Migration history persistence (see `crate::migration::history`).
//!
//! Invariants/assumptions:
//! - Requires a valid Ralph project (with .ralph directory).
//! - `--apply` requires explicit user action (not automatic).
//! - Exit code 1 from `--check` when migrations are pending for CI integration.

use crate::commands::init::gitignore;
use crate::config;
use crate::migration::{self, MigrationCheckResult, MigrationContext};
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;

#[derive(Args)]
#[command(
    about = "Check and apply migrations for config and project files",
    after_long_help = "Examples:
  ralph migrate              # Check for pending migrations
  ralph migrate --check      # Exit with error code if migrations pending (CI)
  ralph migrate --apply      # Apply all pending migrations
  ralph migrate --list       # List all migrations and their status
  ralph migrate status       # Show detailed migration status
"
)]
pub struct MigrateArgs {
    /// Check for pending migrations without applying them (exit 1 if any pending).
    #[arg(long, conflicts_with = "apply")]
    pub check: bool,

    /// Apply pending migrations.
    #[arg(long, conflicts_with = "check")]
    pub apply: bool,

    /// List all migrations and their status.
    #[arg(long, conflicts_with_all = ["check", "apply"])]
    pub list: bool,

    /// Force apply migrations even if already applied (dangerous).
    #[arg(long, requires = "apply")]
    pub force: bool,

    /// Subcommand for more detailed operations.
    #[command(subcommand)]
    pub command: Option<MigrateCommand>,
}

#[derive(Subcommand)]
pub enum MigrateCommand {
    /// Show detailed migration status.
    Status,
}

/// Handle the migrate command.
pub fn handle_migrate(args: MigrateArgs) -> Result<()> {
    // Handle subcommands first
    if let Some(MigrateCommand::Status) = args.command {
        return show_migration_status();
    }

    // Handle flags
    if args.list {
        return list_migrations();
    }

    if args.apply {
        return apply_migrations(args.force);
    }

    if args.check {
        return check_migrations();
    }

    // Default: show pending migrations
    show_pending_migrations()
}

/// Check for pending migrations and exit with error code if any found.
fn check_migrations() -> Result<()> {
    let resolved = config::resolve_from_cwd().context("resolve configuration")?;
    let ctx = MigrationContext::from_resolved(&resolved).context("create migration context")?;

    match migration::check_migrations(&ctx)? {
        MigrationCheckResult::Current => {
            println!("{}", "✓ No pending migrations".green());
            Ok(())
        }
        MigrationCheckResult::Pending(migrations) => {
            println!(
                "{}",
                format!("✗ {} pending migration(s) found", migrations.len()).red()
            );
            for migration in &migrations {
                println!("  - {}: {}", migration.id.yellow(), migration.description);
            }
            println!("\nRun {} to apply them.", "ralph migrate --apply".cyan());
            std::process::exit(1);
        }
    }
}

/// Show pending migrations without exiting with error code.
fn show_pending_migrations() -> Result<()> {
    let resolved = config::resolve_from_cwd().context("resolve configuration")?;
    let ctx = MigrationContext::from_resolved(&resolved).context("create migration context")?;

    match migration::check_migrations(&ctx)? {
        MigrationCheckResult::Current => {
            println!("{}", "✓ No pending migrations".green());
            println!("\nYour project is up to date!");
        }
        MigrationCheckResult::Pending(migrations) => {
            println!(
                "{}",
                format!("Found {} pending migration(s):", migrations.len()).yellow()
            );
            println!();
            for migration in &migrations {
                println!("  {} {}", "•".cyan(), migration.id.bold());
                println!("    {}", migration.description);
                println!();
            }
            println!("Run {} to apply them.", "ralph migrate --apply".cyan());
        }
    }

    Ok(())
}

/// List all migrations with their status.
fn list_migrations() -> Result<()> {
    let resolved = config::resolve_from_cwd().context("resolve configuration")?;
    let ctx = MigrationContext::from_resolved(&resolved).context("create migration context")?;

    let migrations = migration::list_migrations(&ctx);

    if migrations.is_empty() {
        println!("No migrations defined.");
        return Ok(());
    }

    println!("{}", "Available migrations:".bold());
    println!();

    for status in &migrations {
        let status_icon = if status.applied {
            "✓".green()
        } else if status.applicable {
            "○".yellow()
        } else {
            "-".dimmed()
        };

        let status_text = if status.applied {
            "applied".green()
        } else if status.applicable {
            "pending".yellow()
        } else {
            "not applicable".dimmed()
        };

        println!(
            "  {} {} ({})",
            status_icon,
            status.migration.id.bold(),
            status_text
        );
        println!("    {}", status.migration.description);
        println!();
    }

    let applied_count = migrations.iter().filter(|m| m.applied).count();
    let pending_count = migrations
        .iter()
        .filter(|m| !m.applied && m.applicable)
        .count();

    println!(
        "{} applied, {} pending, {} not applicable",
        applied_count.to_string().green(),
        pending_count.to_string().yellow(),
        (migrations.len() - applied_count - pending_count)
            .to_string()
            .dimmed()
    );

    Ok(())
}

/// Apply all pending migrations.
fn apply_migrations(force: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd().context("resolve configuration")?;
    let mut ctx = MigrationContext::from_resolved(&resolved).context("create migration context")?;

    // Check what migrations would be applied
    let pending = match migration::check_migrations(&ctx)? {
        MigrationCheckResult::Current => {
            println!("{}", "✓ No pending migrations to apply".green());
            return Ok(());
        }
        MigrationCheckResult::Pending(migrations) => migrations,
    };

    if force {
        println!(
            "{}",
            "⚠ Force mode enabled: Will re-apply already applied migrations".yellow()
        );
    }

    println!(
        "{}",
        format!("Will apply {} migration(s):", pending.len()).cyan()
    );
    println!();
    for migration in &pending {
        println!("  - {}: {}", migration.id.yellow(), migration.description);
    }
    println!();

    // Confirm with user
    if !force {
        print!("{} ", "Apply these migrations? [y/N]:".bold());
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    println!();

    // Apply migrations
    let applied = migration::apply_all_migrations(&mut ctx).context("apply migrations")?;

    if applied.is_empty() {
        println!("{}", "No migrations were applied".yellow());
    } else {
        println!(
            "{}",
            format!("✓ Successfully applied {} migration(s)", applied.len()).green()
        );
        for id in applied {
            println!("  {} {}", "✓".green(), id);
        }
    }

    // Apply gitignore migration for JSON to JSONC patterns
    match gitignore::migrate_json_to_jsonc_gitignore(&ctx.repo_root) {
        Ok(true) => {
            println!("{}", "✓ Updated .gitignore for JSONC patterns".green());
        }
        Ok(false) => {
            log::debug!(".gitignore JSON to JSONC migration not needed or already up to date");
        }
        Err(e) => {
            eprintln!(
                "{}",
                format!("⚠ Warning: Failed to update .gitignore for JSONC: {}", e).yellow()
            );
        }
    }

    Ok(())
}

/// Show detailed migration status.
fn show_migration_status() -> Result<()> {
    let resolved = config::resolve_from_cwd().context("resolve configuration")?;
    let ctx = MigrationContext::from_resolved(&resolved).context("create migration context")?;

    println!("{}", "Migration Status".bold());
    println!();

    // Show migration history info
    println!("{}", "History:".bold());
    println!(
        "  Location: {}",
        migration::history::migration_history_path(&ctx.repo_root).display()
    );
    println!(
        "  Applied migrations: {}",
        ctx.migration_history.applied_migrations.len()
    );
    println!();

    // Show pending migrations
    match migration::check_migrations(&ctx)? {
        MigrationCheckResult::Current => {
            println!("{}", "Pending migrations: None".green());
        }
        MigrationCheckResult::Pending(migrations) => {
            println!(
                "{} {}",
                "Pending migrations:".yellow(),
                format!("({})", migrations.len()).yellow()
            );
            for migration in migrations {
                println!("  - {}: {}", migration.id.yellow(), migration.description);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_args_default_values() {
        // Test that the struct can be created with default values
        let args = MigrateArgs {
            check: false,
            apply: false,
            list: false,
            force: false,
            command: None,
        };
        assert!(!args.check);
        assert!(!args.apply);
        assert!(!args.list);
        assert!(!args.force);
    }

    #[test]
    fn migrate_args_with_check_enabled() {
        let args = MigrateArgs {
            check: true,
            apply: false,
            list: false,
            force: false,
            command: None,
        };
        assert!(args.check);
    }

    #[test]
    fn migrate_args_with_apply_and_force() {
        let args = MigrateArgs {
            check: false,
            apply: true,
            list: false,
            force: true,
            command: None,
        };
        assert!(args.apply);
        assert!(args.force);
    }

    #[test]
    fn migrate_command_status_variant() {
        let cmd = MigrateCommand::Status;
        assert!(matches!(cmd, MigrateCommand::Status));
    }
}
