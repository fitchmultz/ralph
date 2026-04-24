//! Git LFS filter validation.
//!
//! Purpose:
//! - Git LFS filter validation.
//!
//! Responsibilities:
//! - Validate `filter.lfs.smudge` and `filter.lfs.clean` git config entries.
//! - Normalize missing-key cases into misconfiguration instead of hard failure.
//!
//! Not handled here:
//! - `git lfs status` parsing or pointer validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `git config --get` exit code `1` with empty stderr indicates a missing config key.

use super::types::LfsFilterStatus;
use crate::git::error::{GitError, git_output};
use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn validate_lfs_filters(repo_root: &Path) -> Result<LfsFilterStatus, GitError> {
    fn parse_config_get_output(
        args: &str,
        output: &std::process::Output,
    ) -> Result<(bool, Option<String>), GitError> {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Ok((true, Some(value)));
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if !stderr.is_empty() {
            return Err(GitError::CommandFailed {
                args: args.to_string(),
                code: output.status.code(),
                stderr: stderr.to_string(),
            });
        }

        Ok((false, None))
    }

    let smudge_output = git_output(repo_root, &["config", "--get", "filter.lfs.smudge"])
        .with_context(|| {
            format!(
                "run git config --get filter.lfs.smudge in {}",
                repo_root.display()
            )
        })?;
    let clean_output = git_output(repo_root, &["config", "--get", "filter.lfs.clean"])
        .with_context(|| {
            format!(
                "run git config --get filter.lfs.clean in {}",
                repo_root.display()
            )
        })?;

    let (smudge_installed, smudge_value) =
        parse_config_get_output("config --get filter.lfs.smudge", &smudge_output)?;
    let (clean_installed, clean_value) =
        parse_config_get_output("config --get filter.lfs.clean", &clean_output)?;

    Ok(LfsFilterStatus {
        smudge_installed,
        clean_installed,
        smudge_value,
        clean_value,
    })
}
