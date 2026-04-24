//! `ralph tutorial` command: Clap types and handler.
//!
//! Purpose:
//! - `ralph tutorial` command: Clap types and handler.
//!
//! Responsibilities:
//! - Parse CLI arguments for the tutorial command.
//! - Determine interactive vs non-interactive mode.
//! - Delegate to the tutorial command implementation.
//!
//! Not handled here:
//! - Actual tutorial phase logic (see `crate::commands::tutorial`).
//! - Sandbox creation (see `crate::commands::tutorial::sandbox`).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;
use clap::Args;

use crate::commands::tutorial as tutorial_cmd;

/// Determine if both stdin and stdout are TTYs (interactive terminal).
fn is_tty() -> bool {
    atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout)
}

/// Resolve interactive mode based on explicit flags and TTY detection.
fn resolve_interactive_mode(explicit_non_interactive: bool) -> bool {
    if explicit_non_interactive {
        false
    } else {
        is_tty()
    }
}

/// Handle the tutorial command.
pub fn handle_tutorial(args: TutorialArgs) -> Result<()> {
    let interactive = resolve_interactive_mode(args.non_interactive);

    tutorial_cmd::run_tutorial(tutorial_cmd::TutorialOptions {
        interactive,
        keep_sandbox: args.keep_sandbox,
    })
}

#[derive(Args)]
#[command(
    about = "Run interactive tutorial for Ralph onboarding",
    after_long_help = "Examples:\n  ralph tutorial\n  ralph tutorial --keep-sandbox\n  ralph tutorial --non-interactive\n\nThe tutorial creates a temporary sandbox project and walks you through:\n  1. Initializing Ralph in a project\n  2. Creating your first task\n  3. Running a task (dry-run preview)\n  4. Reviewing the results\n\nUse --keep-sandbox to preserve the sandbox directory after the tutorial.\nUse --non-interactive for automated testing or CI environments."
)]
pub struct TutorialArgs {
    /// Keep the sandbox directory after tutorial completion.
    #[arg(long)]
    pub keep_sandbox: bool,

    /// Skip interactive prompts (for testing/CI).
    #[arg(long)]
    pub non_interactive: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_interactive_mode_explicit_non_interactive() {
        let result = resolve_interactive_mode(true);
        assert!(!result);
    }

    #[test]
    fn resolve_interactive_mode_auto_detect() {
        let result = resolve_interactive_mode(false);
        // In test environment (non-TTY), should be false
        // In TTY environment, would be true
        assert_eq!(result, is_tty());
    }
}
