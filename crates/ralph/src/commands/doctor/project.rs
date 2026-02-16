//! Project environment checks for the doctor command.
//!
//! Responsibilities:
//! - Verify Makefile existence and CI target
//! - Check .gitignore for sensitive log entries
//! - Apply safe auto-fixes for gitignore issues
//!
//! Not handled here:
//! - Build system validation
//! - Dependency management
//!
//! Invariants/assumptions:
//! - .ralph/logs/ should always be gitignored to prevent secret leakage
//! - Auto-fixes are conservative and idempotent

use crate::commands::doctor::types::{CheckResult, DoctorReport};
use crate::config;
use std::fs;

pub(crate) fn check_project(
    report: &mut DoctorReport,
    resolved: &config::Resolved,
    auto_fix: bool,
) {
    // Check Makefile
    let makefile_path = resolved.repo_root.join("Makefile");
    if makefile_path.exists() {
        report.add(CheckResult::success(
            "project",
            "makefile",
            "Makefile found",
        ));
        match fs::read_to_string(&makefile_path) {
            Ok(content) => {
                if content.contains("ci:") {
                    report.add(CheckResult::success(
                        "project",
                        "ci_target",
                        "Makefile has 'ci' target",
                    ));
                } else {
                    report.add(CheckResult::warning(
                        "project",
                        "ci_target",
                        "Makefile exists but missing 'ci' target",
                        false,
                        Some("Add a 'ci' target to your Makefile for automated checks"),
                    ));
                }
            }
            Err(e) => {
                report.add(CheckResult::error(
                    "project",
                    "makefile_read",
                    &format!("failed to read Makefile: {}", e),
                    false,
                    Some("Check file permissions"),
                ));
            }
        }
    } else {
        report.add(CheckResult::error(
            "project",
            "makefile",
            "Makefile missing in repo root",
            false,
            Some("Create a Makefile with a 'ci' target"),
        ));
    }

    // Check .gitignore for .ralph/logs/ entry
    check_gitignore_ralph_logs(report, resolved, auto_fix);
}

/// Check if `.ralph/logs/` is in repo root `.gitignore`.
///
/// This check inspects the repo-local `.gitignore` file content directly
/// (not using `git check-ignore`, which would incorrectly pass on machines
/// with global excludes).
pub(crate) fn check_gitignore_ralph_logs(
    report: &mut DoctorReport,
    resolved: &config::Resolved,
    auto_fix: bool,
) {
    let gitignore_path = resolved.repo_root.join(".gitignore");

    // Check if .gitignore exists
    let content = if gitignore_path.exists() {
        match fs::read_to_string(&gitignore_path) {
            Ok(c) => c,
            Err(e) => {
                report.add(CheckResult::error(
                    "project",
                    "gitignore_ralph_logs",
                    &format!("failed to read .gitignore: {}", e),
                    false,
                    Some("Check file permissions"),
                ));
                return;
            }
        }
    } else {
        String::new()
    };

    // Check if .ralph/logs/ is already ignored
    let has_logs_entry = content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == ".ralph/logs/" || trimmed == ".ralph/logs"
    });

    if has_logs_entry {
        report.add(CheckResult::success(
            "project",
            "gitignore_ralph_logs",
            ".gitignore contains .ralph/logs/ (debug logs will not be committed)",
        ));
        return;
    }

    // Missing entry - this is a high-severity issue because debug logs may contain secrets
    let fix_available = true;
    let mut result = CheckResult::error(
        "project",
        "gitignore_ralph_logs",
        ".gitignore missing ignore rule for .ralph/logs/ (debug logs may contain secrets)",
        fix_available,
        Some("Add this to your repo root .gitignore:\n\n.ralph/logs/\n"),
    );

    if auto_fix && fix_available {
        match crate::commands::init::gitignore::ensure_ralph_gitignore_entries(&resolved.repo_root)
        {
            Ok(()) => {
                // Verify the fix was applied by re-reading the file
                match fs::read_to_string(&gitignore_path) {
                    Ok(new_content) => {
                        let now_has_entry = new_content.lines().any(|line| {
                            let trimmed = line.trim();
                            trimmed == ".ralph/logs/" || trimmed == ".ralph/logs"
                        });
                        if now_has_entry {
                            log::info!("Auto-fixed: added .ralph/logs/ to .gitignore");
                            // Convert to success since fix was applied
                            result = CheckResult::success(
                                "project",
                                "gitignore_ralph_logs",
                                ".gitignore now contains .ralph/logs/ (auto-fixed)",
                            )
                            .with_fix_applied(true);
                        } else {
                            result = result.with_fix_applied(false);
                        }
                    }
                    Err(_) => {
                        result = result.with_fix_applied(false);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to auto-fix .gitignore: {}", e);
                result = result.with_fix_applied(false);
            }
        }
    }

    report.add(result);
}
