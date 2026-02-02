//! Path filtering and pattern matching for the watch command.
//!
//! Responsibilities:
//! - Filter file paths from watch events based on patterns and ignore rules.
//! - Match filenames against glob patterns using globset.
//!
//! Not handled here:
//! - File watching or event handling (see `event_loop.rs`).
//! - Comment detection (see `comments.rs`).
//!
//! Invariants/assumptions:
//! - Directories are always skipped.
//! - Ignore patterns take precedence over include patterns.
//! - Common ignore directories (target/, node_modules/, .git/, etc.) are hardcoded.

use crate::commands::watch::types::WatchOptions;
use notify::Event;
use std::path::{Path, PathBuf};

/// Get relevant file paths from a watch event.
pub fn get_relevant_paths(event: &Event, opts: &WatchOptions) -> Option<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = event
        .paths
        .iter()
        .filter(|p| should_process_file(p, &opts.patterns, &opts.ignore_patterns))
        .cloned()
        .collect();

    if paths.is_empty() { None } else { Some(paths) }
}

/// Check if a file should be processed based on patterns and ignore rules.
pub fn should_process_file(path: &Path, patterns: &[String], ignore_patterns: &[String]) -> bool {
    // Skip directories
    if path.is_dir() {
        return false;
    }

    // Check if file matches any pattern
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Check ignore patterns first
    for ignore in ignore_patterns {
        if matches_pattern(file_name, ignore) {
            return false;
        }
    }

    // Check if in common ignore directories
    let path_str = path.to_string_lossy();
    let ignore_dirs = [
        "/target/",
        "/node_modules/",
        "/.git/",
        "/vendor/",
        "/.ralph/",
    ];
    for dir in &ignore_dirs {
        if path_str.contains(dir) {
            return false;
        }
    }

    // Check if file matches any pattern
    patterns.iter().any(|p| matches_pattern(file_name, p))
}

/// Match a filename against a glob pattern using globset.
///
/// Supports standard glob syntax:
/// - `*` matches any sequence of characters (except `/`)
/// - `?` matches any single character
/// - `[abc]` matches any character in the set
/// - `[a-z]` matches any character in the range
pub fn matches_pattern(name: &str, pattern: &str) -> bool {
    globset::Glob::new(pattern)
        .map(|g| g.compile_matcher().is_match(name))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pattern_basic() {
        assert!(matches_pattern("test.rs", "*.rs"));
        assert!(matches_pattern("test.rs", "test.*"));
        assert!(!matches_pattern("test.py", "*.rs"));
    }

    #[test]
    fn matches_pattern_question() {
        assert!(matches_pattern("test.rs", "t??t.rs"));
        assert!(!matches_pattern("test.rs", "t?t.rs"));
    }

    #[test]
    fn matches_pattern_regex_metacharacters() {
        // Character class patterns - these would break with the old regex-based implementation
        // Note: *.[rs] matches files ending in .r or .s (single char), not .rs
        assert!(matches_pattern("test.r", "*.[rs]"));
        assert!(matches_pattern("test.s", "*.[rs]"));
        assert!(!matches_pattern("test.rs", "*.[rs]"));
        assert!(!matches_pattern("test.py", "*.[rs]"));

        // Plus sign in filename - + is literal in glob, not a regex quantifier
        assert!(matches_pattern("file+1.txt", "file+*.txt"));
        assert!(matches_pattern("file+123.txt", "file+*.txt"));

        // Parentheses in filename - () are literal in glob, not regex groups
        assert!(matches_pattern("test(1).rs", "test(*).rs"));
        assert!(matches_pattern("test(backup).rs", "test(*).rs"));

        // Dollar signs in filename - $ is literal in glob, not regex anchor
        assert!(matches_pattern("test.$$$", "test.*"));
        assert!(matches_pattern("file.$$$.txt", "file.*.txt"));

        // Caret in filename - ^ is literal in glob, not regex anchor
        assert!(matches_pattern("file^name.txt", "file^name.txt"));
        assert!(matches_pattern("file^name.txt", "file*.txt"));
    }

    #[test]
    fn matches_pattern_character_classes() {
        // Range patterns
        assert!(matches_pattern("file1.txt", "file[0-9].txt"));
        assert!(matches_pattern("file5.txt", "file[0-9].txt"));
        assert!(matches_pattern("file9.txt", "file[0-9].txt"));
        assert!(!matches_pattern("filea.txt", "file[0-9].txt"));

        // Multiple character classes
        assert!(matches_pattern("test_a.rs", "test_[a-z].rs"));
        assert!(matches_pattern("test_z.rs", "test_[a-z].rs"));
        assert!(!matches_pattern("test_1.rs", "test_[a-z].rs"));
    }

    #[test]
    fn matches_pattern_edge_cases() {
        // Empty pattern should only match empty string
        assert!(matches_pattern("", ""));
        assert!(!matches_pattern("test.rs", ""));

        // Invalid glob patterns should return false (not panic)
        // Unclosed character class is invalid in globset
        assert!(!matches_pattern("test.rs", "*.[rs"));

        // Just wildcards
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("a", "?"));
        assert!(!matches_pattern("ab", "?"));
    }
}
