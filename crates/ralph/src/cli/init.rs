//! `ralph init` command: Clap types and handler.
//!
//! Responsibilities:
//! - Parse CLI arguments for the init command.
//! - Determine interactive vs non-interactive mode based on flags and TTY detection.
//! - Delegate to the init command implementation.
//!
//! Not handled here:
//! - Actual file creation logic (see `crate::commands::init`).
//! - Interactive wizard implementation (see `crate::commands::init`).
//!
//! Invariants/assumptions:
//! - `--interactive` and `--non-interactive` are mutually exclusive.
//! - TTY detection requires both stdin and stdout to be TTYs for interactive mode.
//! - `--interactive` fails fast if stdin/stdout are not usable TTYs.

use anyhow::{Context, Result};
use clap::Args;

use crate::{commands::init as init_cmd, config};

/// Determine if both stdin and stdout are TTYs (interactive terminal).
///
/// Both streams must be TTYs for interactive prompting to work correctly.
fn is_tty() -> bool {
    atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout)
}

/// Resolve interactive mode based on explicit flags and TTY detection.
///
/// Behavior:
/// - `--interactive` explicitly enables interactive mode; errors if no TTY.
/// - `--non-interactive` explicitly disables interactive mode.
/// - Auto-detects based on TTY when neither flag is provided.
///
/// Returns Ok(true) for interactive mode, Ok(false) for non-interactive.
/// Returns Err if `--interactive` is requested without a usable TTY.
fn resolve_interactive_mode(
    explicit_interactive: bool,
    explicit_non_interactive: bool,
) -> Result<bool> {
    match (explicit_interactive, explicit_non_interactive) {
        (true, _) => {
            // Explicit --interactive: require TTY
            if is_tty() {
                Ok(true)
            } else {
                anyhow::bail!(
                    "Interactive mode requested (--interactive) but stdin/stdout is not a TTY. \
                     Use --non-interactive for CI/piped environments."
                )
            }
        }
        (_, true) => {
            // Explicit --non-interactive
            Ok(false)
        }
        (false, false) => {
            // Auto-detect: require both stdin and stdout TTY
            Ok(is_tty())
        }
    }
}

pub fn handle_init(args: InitArgs, force_lock: bool) -> Result<()> {
    let resolved = config::resolve_from_cwd()?;

    // Handle --check mode: verify README is current and exit
    // This runs before interactive resolution so it works in non-TTY environments
    if args.check {
        let check_result = init_cmd::check_readme_current(&resolved)?;
        match check_result {
            init_cmd::ReadmeCheckResult::Current(version) => {
                log::info!("readme: current (version {})", version);
                return Ok(());
            }
            init_cmd::ReadmeCheckResult::Outdated {
                current_version,
                embedded_version,
            } => {
                log::warn!(
                    "readme: outdated (current version {}, embedded version {})",
                    current_version,
                    embedded_version
                );
                log::info!("Run 'ralph init --update-readme' to update");
                std::process::exit(1);
            }
            init_cmd::ReadmeCheckResult::Missing => {
                log::warn!("readme: missing (would be created on normal init)");
                std::process::exit(1);
            }
            init_cmd::ReadmeCheckResult::NotApplicable => {
                log::info!("readme: not applicable (prompts don't reference README)");
                return Ok(());
            }
        }
    }

    // Determine interactive mode: explicit flags override TTY detection
    let interactive = resolve_interactive_mode(args.interactive, args.non_interactive)
        .with_context(|| {
            "Failed to determine interactive mode. \
             Use --non-interactive for CI/piped environments."
        })?;

    let report = init_cmd::run_init(
        &resolved,
        init_cmd::InitOptions {
            force: args.force,
            force_lock,
            interactive,
            update_readme: args.update_readme,
        },
    )?;

    fn report_status(label: &str, status: init_cmd::FileInitStatus, path: &std::path::Path) {
        match status {
            init_cmd::FileInitStatus::Created => {
                log::info!("{}: created ({})", label, path.display())
            }
            init_cmd::FileInitStatus::Valid => {
                log::info!("{}: exists (valid) ({})", label, path.display())
            }
            init_cmd::FileInitStatus::Updated => {
                log::info!("{}: updated ({})", label, path.display())
            }
        }
    }

    report_status("queue", report.queue_status, &resolved.queue_path);
    report_status("done", report.done_status, &resolved.done_path);
    if let Some((status, version_info)) = report.readme_status {
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        match status {
            init_cmd::FileInitStatus::Created => {
                if let Some(version) = version_info {
                    log::info!(
                        "readme: created (version {}) ({})",
                        version,
                        readme_path.display()
                    );
                } else {
                    log::info!("readme: created ({})", readme_path.display());
                }
            }
            init_cmd::FileInitStatus::Valid => {
                if let Some(version) = version_info {
                    log::info!(
                        "readme: exists (version {}) ({})",
                        version,
                        readme_path.display()
                    );
                } else {
                    log::info!("readme: exists (valid) ({})", readme_path.display());
                }
            }
            init_cmd::FileInitStatus::Updated => {
                if let Some(version) = version_info {
                    log::info!(
                        "readme: updated (version {}) ({})",
                        version,
                        readme_path.display()
                    );
                } else {
                    log::info!("readme: updated ({})", readme_path.display());
                }
            }
        }
    }
    if let Some(path) = resolved.project_config_path.as_ref() {
        report_status("config", report.config_status, path);
    } else {
        log::info!("config: unavailable");
    }
    Ok(())
}

#[derive(Args)]
#[command(
    about = "Bootstrap Ralph files in the current repository",
    after_long_help = "Examples:\n  ralph init\n  ralph init --force\n  ralph init --interactive\n  ralph init --non-interactive\n  ralph init --update-readme\n  ralph init --check"
)]
pub struct InitArgs {
    /// Overwrite existing files if they already exist.
    #[arg(long)]
    pub force: bool,

    /// Run interactive onboarding wizard (requires stdin+stdout TTY).
    #[arg(short, long)]
    pub interactive: bool,

    /// Skip interactive prompts even if running in a TTY.
    #[arg(long, conflicts_with = "interactive")]
    pub non_interactive: bool,

    /// Update README if it exists (force overwrite with latest template).
    #[arg(long)]
    pub update_readme: bool,

    /// Check if README is current and exit (exit 0 if current, 1 if outdated/missing).
    #[arg(long)]
    pub check: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_interactive_mode_explicit_non_interactive() {
        // --non-interactive should always return false
        let result = resolve_interactive_mode(false, true);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn resolve_interactive_mode_explicit_interactive_without_tty() {
        // --interactive without TTY should fail
        // In a test environment (non-TTY), this should return an error
        let result = resolve_interactive_mode(true, false);
        // In non-TTY test environment, this should fail
        if !is_tty() {
            assert!(result.is_err());
        } else {
            assert!(result.is_ok());
            assert!(result.unwrap());
        }
    }

    #[test]
    fn resolve_interactive_mode_auto_detect() {
        // Auto-detect should return false in non-TTY environment
        let result = resolve_interactive_mode(false, false);
        assert!(result.is_ok());
        // In test environment (non-TTY), should be false
        // In TTY environment, would be true
        assert_eq!(result.unwrap(), is_tty());
    }

    #[test]
    fn resolve_interactive_mode_explicit_interactive_wins_over_non_interactive() {
        // If both are true (shouldn't happen due to clap conflicts, but test logic)
        // --interactive takes precedence
        let result = resolve_interactive_mode(true, true);
        // In non-TTY test environment, this should fail
        // In TTY environment, should succeed with true
        if !is_tty() {
            assert!(result.is_err());
        } else {
            assert!(result.is_ok());
            assert!(result.unwrap());
        }
    }
}
