//! Shell completion script generation for Ralph CLI.
//!
//! Purpose:
//! - Shell completion script generation for Ralph CLI.
//!
//! Responsibilities:
//! - Generate shell completion scripts for supported shells (bash, zsh, fish, PowerShell, Elvish).
//! - Provide a CLI command to output completion scripts to stdout.
//!
//! Not handled here:
//! - Installation of completion scripts to system directories (user responsibility).
//! - Runtime shell detection or automatic configuration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Completion scripts are generated using clap_complete and written to stdout.
//! - Users redirect output to appropriate shell-specific completion directories.

use anyhow::Result;
use clap::{CommandFactory, ValueEnum};
use clap_complete::{Shell as ClapShell, generate};

/// Arguments for the completions command.
#[derive(clap::Args, Debug)]
pub struct CompletionsArgs {
    /// The shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Supported shells for completion generation.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Shell {
    /// Bash shell completions
    Bash,
    /// Zsh shell completions
    Zsh,
    /// Fish shell completions
    Fish,
    /// PowerShell completions
    #[value(name = "powershell")]
    PowerShell,
    /// Elvish shell completions
    Elvish,
}

impl From<Shell> for ClapShell {
    fn from(shell: Shell) -> Self {
        match shell {
            Shell::Bash => ClapShell::Bash,
            Shell::Zsh => ClapShell::Zsh,
            Shell::Fish => ClapShell::Fish,
            Shell::PowerShell => ClapShell::PowerShell,
            Shell::Elvish => ClapShell::Elvish,
        }
    }
}

/// Generate and print shell completion script for the specified shell.
///
/// The completion script is written to stdout. Users should redirect
/// the output to the appropriate location for their shell.
pub fn handle_completions(args: CompletionsArgs) -> Result<()> {
    let mut cmd = crate::cli::Cli::command();
    let shell: ClapShell = args.shell.into();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ValueEnum;

    #[test]
    fn shell_enum_parses_bash() {
        let shell = Shell::from_str("bash", true).expect("parse bash");
        assert!(matches!(shell, Shell::Bash));
    }

    #[test]
    fn shell_enum_parses_zsh() {
        let shell = Shell::from_str("zsh", true).expect("parse zsh");
        assert!(matches!(shell, Shell::Zsh));
    }

    #[test]
    fn shell_enum_parses_fish() {
        let shell = Shell::from_str("fish", true).expect("parse fish");
        assert!(matches!(shell, Shell::Fish));
    }

    #[test]
    fn shell_enum_parses_powershell() {
        let shell = Shell::from_str("powershell", true).expect("parse powershell");
        assert!(matches!(shell, Shell::PowerShell));
    }

    #[test]
    fn shell_enum_parses_elvish() {
        let shell = Shell::from_str("elvish", true).expect("parse elvish");
        assert!(matches!(shell, Shell::Elvish));
    }

    #[test]
    fn shell_enum_rejects_invalid() {
        let result = Shell::from_str("invalid", true);
        assert!(result.is_err());
    }

    #[test]
    fn shell_conversion_to_clap_shell() {
        assert!(matches!(ClapShell::from(Shell::Bash), ClapShell::Bash));
        assert!(matches!(ClapShell::from(Shell::Zsh), ClapShell::Zsh));
        assert!(matches!(ClapShell::from(Shell::Fish), ClapShell::Fish));
        assert!(matches!(
            ClapShell::from(Shell::PowerShell),
            ClapShell::PowerShell
        ));
        assert!(matches!(ClapShell::from(Shell::Elvish), ClapShell::Elvish));
    }

    #[test]
    fn handle_completions_generates_non_empty_output() {
        let mut output = Vec::new();
        let mut cmd = crate::cli::Cli::command();
        let bin_name = cmd.get_name().to_string();
        generate(ClapShell::Bash, &mut cmd, bin_name, &mut output);

        assert!(!output.is_empty(), "completion script should not be empty");
        let output_str = String::from_utf8(output).expect("valid UTF-8");
        assert!(
            output_str.contains("ralph"),
            "completion script should reference 'ralph'"
        );
    }
}
