//! Search options for task filtering and searching.
//!
//! Purpose:
//! - Search options for task filtering and searching.
//!
//! Responsibilities:
//! - Define `SearchOptions` struct that unifies parameters for CLI and GUI clients
//!
//! Not handled here:
//! - Actual search/filter implementation (see sibling modules)
//! - Default value logic for scopes (handled by callers)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All boolean flags default to false for conservative/safe behavior
//! - Scopes vec is owned (callers normalize before setting)

/// Options controlling search and filtering behavior.
///
/// This struct unifies the parameters used by both CLI and GUI clients for
/// consistent search semantics across surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchOptions {
    /// Use regular expression matching (default: false, use substring).
    pub use_regex: bool,
    /// Case-sensitive search (default: false, case-insensitive).
    pub case_sensitive: bool,
    /// Use fuzzy matching (default: false, use substring).
    pub use_fuzzy: bool,
    /// Scope filter tokens (default: empty, no scope filter).
    pub scopes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_options_default_values() {
        let opts = SearchOptions::default();
        assert!(!opts.use_regex, "default: substring search");
        assert!(!opts.case_sensitive, "default: case-insensitive");
        assert!(opts.scopes.is_empty(), "default: no scope filter");
    }

    #[test]
    fn search_options_regex_enabled() {
        let opts = SearchOptions {
            use_regex: true,
            case_sensitive: false,
            use_fuzzy: false,
            scopes: vec![],
        };
        assert!(opts.use_regex, "regex enabled");
        assert!(!opts.case_sensitive, "case-insensitive");
        assert!(!opts.use_fuzzy, "fuzzy disabled");
    }

    #[test]
    fn search_options_case_sensitive_enabled() {
        let opts = SearchOptions {
            use_regex: false,
            case_sensitive: true,
            use_fuzzy: false,
            scopes: vec![],
        };
        assert!(!opts.use_regex, "substring search");
        assert!(opts.case_sensitive, "case-sensitive");
        assert!(!opts.use_fuzzy, "fuzzy disabled");
    }

    #[test]
    fn search_options_both_enabled() {
        let opts = SearchOptions {
            use_regex: true,
            case_sensitive: true,
            use_fuzzy: false,
            scopes: vec![],
        };
        assert!(opts.use_regex, "regex enabled");
        assert!(opts.case_sensitive, "case-sensitive");
        assert!(!opts.use_fuzzy, "fuzzy disabled");
    }

    #[test]
    fn search_options_with_scopes() {
        let opts = SearchOptions {
            use_regex: false,
            case_sensitive: false,
            use_fuzzy: false,
            scopes: vec!["crates/ralph".to_string()],
        };
        assert!(!opts.use_regex, "substring search");
        assert!(!opts.case_sensitive, "case-insensitive");
        assert!(!opts.use_fuzzy, "fuzzy disabled");
        assert_eq!(opts.scopes.len(), 1, "one scope filter");
        assert_eq!(opts.scopes[0], "crates/ralph");
    }

    #[test]
    fn search_options_fuzzy_enabled() {
        let opts = SearchOptions {
            use_regex: false,
            case_sensitive: false,
            use_fuzzy: true,
            scopes: vec![],
        };
        assert!(!opts.use_regex, "substring search");
        assert!(!opts.case_sensitive, "case-insensitive");
        assert!(opts.use_fuzzy, "fuzzy enabled");
    }
}
