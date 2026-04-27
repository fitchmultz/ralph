//! Automatic startup health checks with auto-fix and migration prompts.
//!
//! Purpose:
//! - Automatic startup health checks with auto-fix and migration prompts.
//!
//! Responsibilities:
//! - Run lightweight health checks on Ralph startup for key commands.
//! - Auto-update README.md when embedded template is newer (no prompt).
//! - Detect and prompt for config migrations (deprecated keys, unknown keys).
//! - Support --auto-fix flag to auto-approve all migrations without prompting.
//! - Support --no-sanity-checks flag to skip sanity health checks.
//! - Support non-interactive mode to skip all prompts (for CI/piped runs).
//!
//! Not handled here:
//! - Deep validation (git, runners, queue structure) - that's `ralph doctor`.
//! - GUI app flows.
//! - Network connectivity checks.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Sanity checks are fast and lightweight.
//! - README auto-update is automatic (users shouldn't edit this file manually).
//! - Config migrations require user confirmation unless --auto-fix is set.
//! - Unknown config keys prompt for remove/keep/rename action.
//! - Prompts require both stdin and stdout to be TTYs.
//! - If non_interactive is true, all prompts are skipped (use --auto-fix to apply changes).
//! - README sync may also be invoked directly by command routing for agent-facing commands.
//! - Read-only startup sanity mode preserves diagnostics while suppressing all writes/prompts.

mod migrations;
mod readme;
mod unknown_keys;

use crate::config::Resolved;
use crate::migration::MigrationContext;
use crate::outpututil;
use anyhow::{Context, Result};
use std::io::{self, Write};

// Re-export submodule functions for internal use
pub(crate) use migrations::check_and_handle_migrations;
pub(crate) use readme::check_and_update_readme;
pub(crate) use unknown_keys::check_unknown_keys;

/// Whether a command should refresh `.ralph/README.md` before execution.
///
/// This is intentionally broader than full sanity checks so agent-facing commands
/// always get current project guidance even when migration checks are not run.
pub fn should_refresh_readme_for_command(command: &crate::cli::Command) -> bool {
    use crate::cli;
    matches!(
        command,
        cli::Command::Run(_)
            | cli::Command::Task(_)
            | cli::Command::Scan(_)
            | cli::Command::Prompt(_)
            | cli::Command::Prd(_)
            | cli::Command::Tutorial(_)
    )
}

/// Refresh `.ralph/README.md` if missing/outdated.
///
/// Returns a user-facing status message when a change was applied.
pub fn refresh_readme_if_needed(resolved: &Resolved) -> Result<Option<String>> {
    check_and_update_readme(resolved)
}

/// Startup sanity execution mode for a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupSanityMode {
    /// Skip startup sanity checks.
    None,
    /// Run startup sanity checks in inspect-only mode (no writes or prompts).
    ReadOnly,
    /// Run startup sanity checks with writes/prompts allowed by CLI flags.
    Mutating,
}

/// Whether startup sanity writes are permitted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SanityWritePolicy {
    /// Sanity checks may apply writes based on other options.
    #[default]
    AllowWrites,
    /// Sanity checks must remain read-only.
    ReadOnly,
}

/// Options for controlling sanity check behavior.
#[derive(Debug, Clone, Default)]
pub struct SanityOptions {
    /// Auto-approve all fixes without prompting.
    pub auto_fix: bool,
    /// Skip all sanity checks.
    pub skip: bool,
    /// Skip interactive prompts even if running in a TTY.
    pub non_interactive: bool,
    /// Write policy for sanity checks.
    pub write_policy: SanityWritePolicy,
}

impl SanityOptions {
    /// Whether sanity checks may apply filesystem writes.
    pub fn allows_writes(&self) -> bool {
        matches!(self.write_policy, SanityWritePolicy::AllowWrites)
    }

    /// Check if we can prompt the user for input.
    pub fn can_prompt(&self) -> bool {
        self.allows_writes() && !self.non_interactive && is_tty()
    }
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
pub fn run_sanity_checks(resolved: &Resolved, options: &SanityOptions) -> Result<SanityResult> {
    if options.skip {
        log::debug!("Sanity checks skipped via --no-sanity-checks");
        return Ok(SanityResult::default());
    }

    log::debug!("Running sanity checks...");
    let mut result = SanityResult::default();
    let effective_auto_fix = options.auto_fix && options.allows_writes();
    let effective_non_interactive = options.non_interactive || !options.allows_writes();

    // Check 1: README health
    if options.allows_writes() {
        match check_and_update_readme(resolved) {
            Ok(Some(fix_msg)) => {
                result.auto_fixes.push(fix_msg);
            }
            Ok(None) => {
                log::debug!("README is current");
            }
            Err(e) => {
                return Err(e).context("check/update .ralph/README.md");
            }
        }
    } else {
        match readme::check_readme_without_update(resolved) {
            Ok(Some(message)) => {
                result.needs_attention.push(SanityIssue {
                    severity: IssueSeverity::Warning,
                    message,
                    fix_available: true,
                });
            }
            Ok(None) => {
                log::debug!("README is current");
            }
            Err(e) => {
                return Err(e).context("check/read-only .ralph/README.md");
            }
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

    match check_and_handle_migrations(
        &mut ctx,
        effective_auto_fix,
        effective_non_interactive,
        is_tty,
        prompt_yes_no,
    ) {
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
    match check_unknown_keys(
        resolved,
        effective_auto_fix,
        effective_non_interactive,
        is_tty,
    ) {
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

/// Check if both stdin and stdout are TTYs (interactive terminal).
fn is_tty() -> bool {
    atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout)
}

/// Resolve startup sanity mode for a given command.
pub fn startup_sanity_mode(command: &crate::cli::Command) -> StartupSanityMode {
    use crate::cli;

    match command {
        cli::Command::Run(_) => StartupSanityMode::Mutating,
        cli::Command::Queue(_) => StartupSanityMode::None,
        cli::Command::Doctor(_) => StartupSanityMode::None,
        _ => StartupSanityMode::None,
    }
}

/// Check if sanity checks should run for a given command.
pub fn should_run_sanity_checks(command: &crate::cli::Command) -> bool {
    !matches!(startup_sanity_mode(command), StartupSanityMode::None)
}

/// Report sanity check results to the user.
pub fn report_sanity_results(result: &SanityResult, auto_fix: bool) -> bool {
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
    use clap::Parser;

    #[test]
    fn sanity_options_can_prompt_non_interactive_disables_prompts() {
        let opts = SanityOptions {
            non_interactive: true,
            ..Default::default()
        };
        assert!(!opts.can_prompt());
    }

    #[test]
    fn sanity_options_default_matches_current_tty_state() {
        let opts = SanityOptions::default();
        assert_eq!(opts.can_prompt(), is_tty());
    }

    #[test]
    fn sanity_options_explicit_non_interactive_overrides() {
        let opts = SanityOptions {
            non_interactive: true,
            auto_fix: false,
            skip: false,
            write_policy: SanityWritePolicy::AllowWrites,
        };
        assert!(!opts.can_prompt());
    }

    #[test]
    fn sanity_options_read_only_disables_prompts() {
        let opts = SanityOptions {
            non_interactive: false,
            auto_fix: false,
            skip: false,
            write_policy: SanityWritePolicy::ReadOnly,
        };
        assert!(!opts.can_prompt());
        assert!(!opts.allows_writes());
    }

    #[test]
    fn should_refresh_readme_for_agent_facing_commands() {
        let cli = crate::cli::Cli::parse_from(["ralph", "task", "build", "x"]);
        assert!(should_refresh_readme_for_command(&cli.command));

        let cli = crate::cli::Cli::parse_from(["ralph", "scan", "--focus", "x"]);
        assert!(should_refresh_readme_for_command(&cli.command));

        let cli = crate::cli::Cli::parse_from(["ralph", "run", "one", "--id", "RQ-0001"]);
        assert!(should_refresh_readme_for_command(&cli.command));

        let cli =
            crate::cli::Cli::parse_from(["ralph", "prompt", "task-builder", "--request", "x"]);
        assert!(should_refresh_readme_for_command(&cli.command));
    }

    #[test]
    fn should_not_refresh_readme_for_non_agent_commands() {
        let cli = crate::cli::Cli::parse_from(["ralph", "queue", "list"]);
        assert!(!should_refresh_readme_for_command(&cli.command));

        let cli = crate::cli::Cli::parse_from(["ralph", "version"]);
        assert!(!should_refresh_readme_for_command(&cli.command));

        let cli = crate::cli::Cli::parse_from(["ralph", "completions", "bash"]);
        assert!(!should_refresh_readme_for_command(&cli.command));
    }

    #[test]
    fn startup_sanity_mode_classifies_commands() {
        let cli = crate::cli::Cli::parse_from(["ralph", "run", "one", "--id", "RQ-0001"]);
        assert_eq!(
            startup_sanity_mode(&cli.command),
            StartupSanityMode::Mutating
        );

        let cli = crate::cli::Cli::parse_from(["ralph", "queue", "validate"]);
        assert_eq!(startup_sanity_mode(&cli.command), StartupSanityMode::None);

        let cli = crate::cli::Cli::parse_from(["ralph", "queue", "list"]);
        assert_eq!(startup_sanity_mode(&cli.command), StartupSanityMode::None);
    }
}
