//! Git health checks for the doctor command.
//!
//! Responsibilities:
//! - Verify git binary is available and executable
//! - Check repository validity and upstream configuration
//! - Check Git LFS status and tracked files
//!
//! Not handled here:
//! - Git operations that modify state
//! - Runner configuration checks
//!
//! Invariants/assumptions:
//! - All checks are read-only and non-destructive
//! - Uses git module functions for repository operations

use crate::commands::doctor::types::{CheckResult, DoctorReport};
use crate::config;
use crate::git;
use std::process::Command;

pub(crate) fn check_git(report: &mut DoctorReport, resolved: &config::Resolved) {
    // Check git binary
    if let Err(e) = check_command("git", &["--version"]) {
        report.add(CheckResult::error(
            "git",
            "git_binary",
            &format!("git binary not found or not executable: {}", e),
            false,
            Some("Install git and ensure it's in your PATH"),
        ));
    } else {
        report.add(CheckResult::success(
            "git",
            "git_binary",
            "git binary found",
        ));
    }

    // Check valid git repo
    match git::status_porcelain(&resolved.repo_root) {
        Ok(_) => {
            report.add(CheckResult::success(
                "git",
                "git_repo",
                &format!("valid git repo at {}", resolved.repo_root.display()),
            ));
        }
        Err(e) => {
            report.add(CheckResult::error(
                "git",
                "git_repo",
                &format!("invalid git repo: {}", e),
                false,
                Some("Run 'git init' to initialize a git repository"),
            ));
        }
    }

    // Check upstream configuration
    match git::upstream_ref(&resolved.repo_root) {
        Ok(u) => {
            report.add(CheckResult::success(
                "git",
                "upstream_config",
                &format!("upstream configured: {}", u),
            ));
        }
        Err(e) => {
            report.add(CheckResult::warning(
                "git",
                "upstream_config",
                &format!("no upstream configured: {}", e),
                false,
                Some("Set up a remote upstream with 'git remote add origin <url>'"),
            ));
        }
    }

    // Git LFS Checks
    match git::has_lfs(&resolved.repo_root) {
        Ok(true) => {
            report.add(CheckResult::success("git", "git_lfs", "Git LFS detected"));
            match git::list_lfs_files(&resolved.repo_root) {
                Ok(files) => {
                    if files.is_empty() {
                        log::info!("LFS initialized but no files tracked");
                    } else {
                        report.add(CheckResult::success(
                            "git",
                            "lfs_files",
                            &format!("LFS tracking {} file(s)", files.len()),
                        ));
                    }
                }
                Err(e) => {
                    report.add(CheckResult::warning(
                        "git",
                        "lfs_files",
                        &format!("Failed to list LFS files: {}", e),
                        false,
                        None,
                    ));
                }
            }
        }
        Ok(false) => {
            log::info!("Git LFS not detected");
        }
        Err(e) => {
            report.add(CheckResult::warning(
                "git",
                "git_lfs",
                &format!("LFS check failed: {}", e),
                false,
                None,
            ));
        }
    }
}

fn check_command(bin: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_msg = if stderr.trim().is_empty() {
            format!(
                "command '{}' {:?} failed with exit status: {}",
                bin, args, output.status
            )
        } else {
            format!(
                "command '{}' {:?} failed with exit status {}: {}",
                bin,
                args,
                output.status,
                stderr.trim()
            )
        };
        Err(anyhow::anyhow!(stderr_msg))
    }
}
