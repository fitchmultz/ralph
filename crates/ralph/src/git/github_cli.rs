//! Shared GitHub CLI helpers for git-facing integrations.
//!
//! Purpose:
//! - Shared GitHub CLI helpers for git-facing integrations.
//!
//! Responsibilities:
//! - Provide consistent `gh` command construction with updater prompts disabled.
//! - Run managed `gh` subprocesses with centralized truncation logging.
//! - Provide optional checked execution so GitHub workflows share one success policy.
//! - Share small output-parsing helpers used by PR/issue modules.
//!
//! Not handled here:
//! - PR- or issue-specific command flags and JSON parsing.
//! - Authentication or lifecycle policy decisions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All commands use `GH_NO_UPDATE_NOTIFIER=1`.
//! - Callers choose the appropriate timeout class for the operation.

use anyhow::Result;
use std::path::Path;
use std::process::{Command, Output};

use crate::runutil::{
    ManagedCommand, TimeoutClass, execute_checked_command, execute_managed_command,
};

pub(crate) fn gh_command(repo_root: &Path) -> Command {
    gh_command_in(repo_root)
}

pub(crate) fn gh_command_in(cwd: &Path) -> Command {
    let mut command = Command::new("gh");
    command.current_dir(cwd).env("GH_NO_UPDATE_NOTIFIER", "1");
    command
}

pub(crate) fn extract_first_url(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(ToString::to_string)
}

pub(crate) fn run_gh_command(
    command: Command,
    description: impl Into<String>,
    timeout_class: TimeoutClass,
    truncation_log_label: &str,
) -> Result<Output> {
    execute_managed_command(ManagedCommand::new(command, description, timeout_class))
        .map(|output| {
            log_gh_truncation(
                truncation_log_label,
                output.stdout_truncated,
                output.stderr_truncated,
            );
            output.into_output()
        })
        .map_err(Into::into)
}

pub(crate) fn run_checked_gh_command(
    command: Command,
    description: impl Into<String>,
    timeout_class: TimeoutClass,
    truncation_log_label: &str,
) -> Result<Output> {
    execute_checked_command(ManagedCommand::new(command, description, timeout_class)).map(
        |output| {
            log_gh_truncation(
                truncation_log_label,
                output.stdout_truncated,
                output.stderr_truncated,
            );
            output.into_output()
        },
    )
}

fn log_gh_truncation(truncation_log_label: &str, stdout_truncated: bool, stderr_truncated: bool) {
    if stdout_truncated || stderr_truncated {
        log::debug!("managed {truncation_log_label} capture truncated command output");
    }
}

#[cfg(test)]
mod tests {
    use super::extract_first_url;

    #[test]
    fn extract_first_url_picks_first_url_line() {
        let output = "Starting operation...\nhttps://github.com/org/repo/issues/5\n";
        let url = extract_first_url(output).expect("url");
        assert_eq!(url, "https://github.com/org/repo/issues/5");
    }

    #[test]
    fn extract_first_url_returns_none_when_absent() {
        assert!(extract_first_url("no url here").is_none());
    }
}
