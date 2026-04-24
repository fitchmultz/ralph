//! String normalization helpers for search and filtering.
//!
//! Purpose:
//! - String normalization helpers for search and filtering.
//!
//! Responsibilities:
//! - Normalize strings for consistent comparison (trim + lowercase)
//!
//! Not handled here:
//! - Regex compilation or pattern matching
//! - Fuzzy matching normalization (handled by nucleo_matcher)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All normalized strings are lowercase and trimmed
//! - Empty strings after trimming are filtered out by callers

/// Normalize a string for comparison by trimming and lowercasing.
///
/// Used for both scope and tag normalization since they share
/// the same semantics (case-insensitive matching after trim).
pub fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize("  hello  "), "hello");
        assert_eq!(normalize("\tworld\n"), "world");
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize("HELLO"), "hello");
        assert_eq!(normalize("Hello World"), "hello world");
    }

    #[test]
    fn normalize_empty_string() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("   "), "");
    }
}
