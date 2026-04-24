//! Git-ref validation rules.
//!
//! Purpose:
//! - Git-ref validation rules.
//!
//! Responsibilities:
//! - Validate git branch/ref names used by configuration or runtime helpers.
//! - Return human-readable invalidity reasons instead of boolean results.
//!
//! Not handled here:
//! - Git command execution.
//! - Queue, trust, or agent validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Rules track Ralph's supported subset of `git check-ref-format`.

pub fn git_ref_invalid_reason(branch: &str) -> Option<String> {
    if branch.is_empty() {
        return Some("branch name cannot be empty".to_string());
    }
    if branch.chars().any(|c| c.is_ascii_control() || c == ' ') {
        return Some("branch name cannot contain spaces or control characters".to_string());
    }
    if branch.contains("..") {
        return Some("branch name cannot contain '..'".to_string());
    }
    if branch.contains("@{") {
        return Some("branch name cannot contain '@{{'".to_string());
    }
    if branch.starts_with('.') {
        return Some("branch name cannot start with '.'".to_string());
    }
    if branch.ends_with(".lock") {
        return Some("branch name cannot end with '.lock'".to_string());
    }
    if branch.contains("//") || branch.contains("/.") || branch.ends_with('/') {
        return Some("branch name contains invalid slash/dot pattern".to_string());
    }
    if branch == "@" || branch.starts_with("@/") || branch.contains("/@/") || branch.ends_with("/@")
    {
        return Some("branch name cannot be '@' or contain '@' as a path component".to_string());
    }
    if branch.contains('~') {
        return Some("branch name cannot contain '~'".to_string());
    }
    if branch.contains('^') {
        return Some("branch name cannot contain '^'".to_string());
    }
    if branch.contains(':') {
        return Some("branch name cannot contain ':'".to_string());
    }
    if branch.contains('\\') {
        return Some("branch name cannot contain '\\'".to_string());
    }
    None
}
