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
    use crate::commands::watch::types::{CommentType, WatchOptions};

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

    // =====================================================================
    // get_relevant_paths tests
    // =====================================================================

    #[test]
    fn get_relevant_paths_filters_non_matching_files() {
        use notify::EventKind;

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![
                PathBuf::from("/test/file.rs"),
                PathBuf::from("/test/file.py"),
            ],
            attrs: Default::default(),
        };

        let result = get_relevant_paths(&event, &opts);

        assert!(result.is_some());
        let paths = result.unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("file.rs"));
    }

    #[test]
    fn get_relevant_paths_returns_none_for_empty_match() {
        use notify::EventKind;

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec![],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("/test/file.py")],
            attrs: Default::default(),
        };

        let result = get_relevant_paths(&event, &opts);

        assert!(result.is_none());
    }

    #[test]
    fn get_relevant_paths_applies_ignore_patterns() {
        use notify::EventKind;

        let opts = WatchOptions {
            patterns: vec!["*.rs".to_string()],
            debounce_ms: 100,
            auto_queue: false,
            notify: false,
            ignore_patterns: vec!["*test*".to_string()],
            comment_types: vec![CommentType::Todo],
            paths: vec![PathBuf::from(".")],
            force: false,
            close_removed: false,
        };

        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![
                PathBuf::from("/test/main.rs"),
                PathBuf::from("/test/main_test.rs"),
            ],
            attrs: Default::default(),
        };

        let result = get_relevant_paths(&event, &opts);

        assert!(result.is_some());
        let paths = result.unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].to_string_lossy().contains("main.rs"));
        assert!(!paths[0].to_string_lossy().contains("test.rs"));
    }

    // =====================================================================
    // should_process_file tests
    // =====================================================================

    #[test]
    fn should_process_file_applies_patterns() {
        let path = Path::new("/test/file.rs");

        assert!(should_process_file(path, &["*.rs".to_string()], &[]));
        assert!(!should_process_file(path, &["*.py".to_string()], &[]));
        assert!(should_process_file(
            path,
            &["*.py".to_string(), "*.rs".to_string()],
            &[]
        ));
    }

    #[test]
    fn should_process_file_applies_ignore_patterns() {
        let path = Path::new("/test/file_test.rs");

        // Without ignore pattern, should match
        assert!(should_process_file(path, &["*.rs".to_string()], &[]));

        // With ignore pattern, should not match
        assert!(!should_process_file(
            path,
            &["*.rs".to_string()],
            &["*test*".to_string()]
        ));
    }

    #[test]
    fn should_process_file_ignore_takes_precedence() {
        let path = Path::new("/test/test.rs");

        // Even if path matches include pattern, ignore should win
        assert!(!should_process_file(
            path,
            &["*.rs".to_string()],
            &["test*".to_string()]
        ));
    }

    #[test]
    fn should_process_file_skips_target_directory() {
        let path = Path::new("/project/target/debug/main.rs");

        assert!(!should_process_file(path, &["*.rs".to_string()], &[]));
    }

    #[test]
    fn should_process_file_skips_node_modules() {
        let path = Path::new("/project/node_modules/some-lib/index.js");

        assert!(!should_process_file(path, &["*.js".to_string()], &[]));
    }

    #[test]
    fn should_process_file_skips_git_directory() {
        let path = Path::new("/project/.git/hooks/pre-commit");

        assert!(!should_process_file(path, &["*".to_string()], &[]));
    }

    #[test]
    fn should_process_file_skips_ralph_directory() {
        let path = Path::new("/project/.ralph/queue.json");

        assert!(!should_process_file(path, &["*.json".to_string()], &[]));
    }
}
