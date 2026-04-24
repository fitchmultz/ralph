//! Snapshot normalization helpers for integration tests.
//!
//! Purpose:
//! - Snapshot normalization helpers for integration tests.
//!
//! Responsibilities:
//! - Normalize CLI output so snapshots stay deterministic across terminals and dates.
//! - Bind `insta` settings shared by CLI snapshot suites.
//!
//! Non-scope:
//! - Creating queue fixtures or subprocess execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Date normalization intentionally targets `YYYY-MM-DD` spans only.
//! - ANSI stripping is limited to simple SGR color sequences used by CLI output.

/// Normalize CLI output for stable snapshots.
pub fn normalize_for_snapshot(output: &str) -> String {
    use regex::Regex;

    let mut result = output.to_string();
    result = result.replace("\r\n", "\n");

    let ansi_regex = Regex::new(r"\x1b\[[0-9;]*m").expect("valid regex");
    result = ansi_regex.replace_all(&result, "").to_string();

    let date_regex = Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").expect("valid regex");
    result = date_regex.replace_all(&result, "<DATE>").to_string();

    result
}

/// Bind `insta` settings suitable for CLI snapshots.
pub fn with_insta_settings<T>(f: impl FnOnce() -> T) -> T {
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(f)
}
