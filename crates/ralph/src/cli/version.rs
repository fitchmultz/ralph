//! Version command for Ralph CLI.
//!
//! Purpose:
//! - Version command for Ralph CLI.
//!
//! Responsibilities:
//! - Display version information including package version, git commit, and build timestamp.
//! - Provide both simple and verbose output modes for version details.
//!
//! Not handled here:
//! - Version bumping or release management (see release workflow).
//! - Changelog generation or modification.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Version info is captured at compile time via environment variables set by the build script.
//! - Git info may be unavailable (e.g., building from tarball), in which case it is omitted gracefully.

use anyhow::Result;

/// Arguments for the version command.
#[derive(clap::Args, Debug)]
pub struct VersionArgs {
    /// Show extended build information including git commit and build date
    #[arg(short, long)]
    pub verbose: bool,
}

/// Display version information for Ralph CLI.
///
/// Prints the package version by default. With --verbose, also displays
/// git commit hash and build timestamp when available.
pub fn handle_version(args: VersionArgs) -> Result<()> {
    let pkg_name = env!("CARGO_PKG_NAME");
    let pkg_version = env!("CARGO_PKG_VERSION");

    if args.verbose {
        println!("{} {}", pkg_name, pkg_version);
        println!();
        println!("Build Info:");

        if let Some(git_sha) = option_env!("VERGEN_GIT_SHA") {
            println!("  Git commit: {}", git_sha);
        }

        if let Some(build_timestamp) = option_env!("VERGEN_BUILD_TIMESTAMP") {
            println!("  Build date: {}", build_timestamp);
        }
    } else {
        println!("{} {}", pkg_name, pkg_version);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::Parser;

    #[test]
    fn cli_parses_version_command() {
        let cli = Cli::try_parse_from(["ralph", "version"]).expect("parse");
        match cli.command {
            Command::Version(args) => {
                assert!(!args.verbose);
            }
            _ => panic!("expected version command"),
        }
    }

    #[test]
    fn cli_parses_version_verbose() {
        let cli = Cli::try_parse_from(["ralph", "version", "--verbose"]).expect("parse");
        match cli.command {
            Command::Version(args) => {
                assert!(args.verbose);
            }
            _ => panic!("expected version command"),
        }
    }

    #[test]
    fn cli_parses_version_verbose_short() {
        let cli = Cli::try_parse_from(["ralph", "version", "-v"]).expect("parse");
        match cli.command {
            Command::Version(args) => {
                assert!(args.verbose);
            }
            _ => panic!("expected version command"),
        }
    }

    #[test]
    fn handle_version_default_output() {
        let args = VersionArgs { verbose: false };
        // Should not panic or error
        let result = handle_version(args);
        assert!(result.is_ok());
    }

    #[test]
    fn handle_version_verbose_output() {
        let args = VersionArgs { verbose: true };
        // Should not panic or error
        let result = handle_version(args);
        assert!(result.is_ok());
    }
}
