//! Color option handling for CLI.
//!
//! Purpose:
//! - Color option handling for CLI.
//!
//! Responsibilities:
//! - Define the ColorArg type for CLI argument parsing.
//! - Provide color initialization and global state management.
//!
//! Not handled here:
//! - App-specific color handling (handled outside the CLI).
//! - Direct color output (see outpututil.rs and output/theme.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - NO_COLOR environment variable takes precedence over CLI flags.
//! - Color initialization happens early in main() before any colored output.

use clap::ValueEnum;

/// Color output control options for CLI arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ColorArg {
    /// Auto-detect based on terminal capabilities.
    #[default]
    Auto,
    /// Always use colors.
    Always,
    /// Never use colors.
    Never,
}

impl ColorArg {
    /// Initialize the colored crate based on the color argument.
    ///
    /// This respects the NO_COLOR environment variable if set.
    pub fn init(self) {
        let use_color = match self {
            ColorArg::Auto => {
                // NO_COLOR takes precedence
                if std::env::var("NO_COLOR").is_ok() {
                    false
                } else {
                    // Auto-detect: use color if stdout is a tty
                    atty::is(atty::Stream::Stdout)
                }
            }
            ColorArg::Always => true,
            ColorArg::Never => false,
        };

        colored::control::set_override(use_color);
    }
}

/// Initialize colors from CLI arguments.
///
/// Call this early in main() before any colored output.
///
/// # Arguments
/// * `color` - The color option from CLI arguments
/// * `no_color` - Whether --no-color was specified
///
/// # Behavior
/// - If `--no-color` is set, colors are disabled
/// - If `NO_COLOR` env var is set, colors are disabled
/// - Otherwise, the `--color` option is respected
pub fn init_color(color: ColorArg, no_color: bool) {
    if no_color || std::env::var("NO_COLOR").is_ok() {
        colored::control::set_override(false);
    } else {
        color.init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_arg_variants_exist() {
        // Just verify the variants are accessible
        let _ = ColorArg::Auto;
        let _ = ColorArg::Always;
        let _ = ColorArg::Never;
    }
}
