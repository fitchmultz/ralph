//! Project environment checks for the doctor command.
//!
//! Responsibilities:
//! - Verify CI-gate prerequisites in the current repository.
//! - Check `.gitignore` for sensitive log entries.
//! - Apply safe auto-fixes for gitignore issues.
//!
//! Not handled here:
//! - Build system validation beyond the configured/local CI entrypoint.
//! - Dependency management.
//!
//! Invariants/assumptions:
//! - `.ralph/logs/` should always be gitignored to prevent secret leakage.
//! - Auto-fixes are conservative and idempotent.
//! - Make-based CI gates should narrate CI-blocking reasons through the canonical blocking contract.

use crate::commands::doctor::types::{CheckResult, DoctorReport};
use crate::config;
use crate::contracts::{BlockingReason, BlockingState, BlockingStatus};
use std::fs;

fn doctor_ci_blocked(
    pattern: &str,
    message: impl Into<String>,
    detail: impl Into<String>,
) -> BlockingState {
    BlockingState::new(
        BlockingStatus::Stalled,
        BlockingReason::CiBlocked {
            exit_code: None,
            pattern: Some(pattern.to_string()),
        },
        None,
        message,
        detail,
    )
    .with_observed_at(crate::timeutil::now_utc_rfc3339_or_fallback())
}

pub(crate) fn check_project(
    report: &mut DoctorReport,
    resolved: &config::Resolved,
    auto_fix: bool,
) {
    check_ci_gate_prerequisites(report, resolved);
    check_gitignore_ralph_logs(report, resolved, auto_fix);
}

fn check_ci_gate_prerequisites(report: &mut DoctorReport, resolved: &config::Resolved) {
    let makefile_path = resolved.repo_root.join("Makefile");
    let make_target = ci_gate_make_target(resolved);

    if let Some(target) = make_target.as_deref() {
        if !makefile_path.exists() {
            report.add(
                CheckResult::error(
                    "project",
                    "makefile",
                    "Makefile missing in repo root",
                    false,
                    Some(&format!("Create a Makefile with a '{target}' target")),
                )
                .with_blocking(doctor_ci_blocked(
                    "makefile_missing",
                    "Ralph is stalled because the project CI gate is unavailable.",
                    format!(
                        "The configured CI gate expects a Makefile target '{target}', but {} is missing.",
                        makefile_path.display()
                    ),
                )),
            );
            return;
        }

        report.add(CheckResult::success(
            "project",
            "makefile",
            "Makefile found",
        ));
        match fs::read_to_string(&makefile_path) {
            Ok(content) => {
                if make_target_exists(&content, target) {
                    report.add(CheckResult::success(
                        "project",
                        "ci_target",
                        &format!("Makefile has '{target}' target"),
                    ));
                } else {
                    report.add(
                        CheckResult::error(
                            "project",
                            "ci_target",
                            &format!("Makefile exists but missing '{target}' target"),
                            false,
                            Some(&format!(
                                "Add a '{target}' target to your Makefile for automated checks"
                            )),
                        )
                        .with_blocking(doctor_ci_blocked(
                            "ci_target_missing",
                            "Ralph is stalled because the project CI gate is unavailable.",
                            format!(
                                "The repository Makefile does not define the configured CI target '{target}'."
                            ),
                        )),
                    );
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
        return;
    }

    if makefile_path.exists() {
        report.add(CheckResult::success(
            "project",
            "makefile",
            "Makefile found",
        ));
    } else {
        report.add(CheckResult::warning(
            "project",
            "makefile",
            "Makefile missing in repo root, but the configured CI gate does not require it",
            false,
            Some("Add a Makefile if you want a local make-based CI entrypoint"),
        ));
    }
}

fn ci_gate_make_target(resolved: &config::Resolved) -> Option<String> {
    let ci_gate = resolved.config.agent.ci_gate.as_ref();
    if ci_gate.is_some_and(|ci_gate| !ci_gate.is_enabled()) {
        return None;
    }

    let argv = ci_gate.and_then(|ci_gate| ci_gate.argv.as_ref());
    let Some(argv) = argv else {
        return Some("ci".to_string());
    };
    if argv.first().map(String::as_str) != Some("make") {
        return None;
    }

    argv.iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .cloned()
        .or_else(|| Some("ci".to_string()))
}

fn make_target_exists(content: &str, target: &str) -> bool {
    let needle = format!("{target}:");
    content
        .lines()
        .map(str::trim_start)
        .any(|line| line.starts_with(&needle))
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
            Ok(()) => match fs::read_to_string(&gitignore_path) {
                Ok(new_content) => {
                    let now_has_entry = new_content.lines().any(|line| {
                        let trimmed = line.trim();
                        trimmed == ".ralph/logs/" || trimmed == ".ralph/logs"
                    });
                    if now_has_entry {
                        log::info!("Auto-fixed: added .ralph/logs/ to .gitignore");
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
            },
            Err(e) => {
                log::error!("Failed to auto-fix .gitignore: {}", e);
                result = result.with_fix_applied(false);
            }
        }
    }

    report.add(result);
}
