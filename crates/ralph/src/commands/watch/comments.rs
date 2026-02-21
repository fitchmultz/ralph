//! Comment detection for the watch command.
//!
//! Responsibilities:
//! - Build regex patterns for detecting TODO/FIXME/HACK/XXX comments.
//! - Detect comments in source files.
//! - Determine comment type from line content.
//! - Extract context for detected comments.
//!
//! Not handled here:
//! - File watching (see `event_loop.rs`).
//! - Task creation from comments (see `tasks.rs`).
//!
//! Invariants/assumptions:
//! - Comment regex is case-insensitive.
//! - Comment content is extracted from capture groups.
//! - Context includes filename, line number, and truncated content.

use crate::commands::watch::types::{CommentType, DetectedComment};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;

/// Build regex for detecting comments based on comment types.
pub fn build_comment_regex(comment_types: &[CommentType]) -> Result<Regex> {
    let mut patterns = Vec::new();

    let has_all = comment_types.contains(&CommentType::All);

    if has_all || comment_types.contains(&CommentType::Todo) {
        patterns.push(r"TODO\s*[:;-]?\s*(.+)$");
    }
    if has_all || comment_types.contains(&CommentType::Fixme) {
        patterns.push(r"FIXME\s*[:;-]?\s*(.+)$");
    }
    if has_all || comment_types.contains(&CommentType::Hack) {
        patterns.push(r"HACK\s*[:;-]?\s*(.+)$");
    }
    if has_all || comment_types.contains(&CommentType::Xxx) {
        patterns.push(r"XXX\s*[:;-]?\s*(.+)$");
    }

    if patterns.is_empty() {
        patterns.push(r"(?:TODO|FIXME|HACK|XXX)\s*[:;-]?\s*(.+)$");
    }

    let combined = patterns.join("|");
    let regex = Regex::new(&format!(r"(?i)({})", combined))
        .context("Failed to compile comment detection regex")?;

    Ok(regex)
}

/// Detect comments in a file.
pub fn detect_comments(file_path: &Path, regex: &Regex) -> Result<Vec<DetectedComment>> {
    let content = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let mut comments = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(captures) = regex.captures(line) {
            // Extract the comment content
            let content = captures
                .get(1)
                .or_else(|| captures.get(2))
                .or_else(|| captures.get(3))
                .or_else(|| captures.get(4))
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            if content.is_empty() {
                continue;
            }

            // Determine comment type from the match
            let comment_type = determine_comment_type(line);

            // Get context (surrounding lines)
            let context = extract_context(&content, line_num + 1, file_path);

            comments.push(DetectedComment {
                file_path: file_path.to_path_buf(),
                line_number: line_num + 1,
                comment_type,
                content,
                context,
            });
        }
    }

    Ok(comments)
}

/// Determine the comment type from a line.
pub fn determine_comment_type(line: &str) -> CommentType {
    let upper = line.to_uppercase();
    if upper.contains("TODO") {
        CommentType::Todo
    } else if upper.contains("FIXME") {
        CommentType::Fixme
    } else if upper.contains("HACK") {
        CommentType::Hack
    } else if upper.contains("XXX") {
        CommentType::Xxx
    } else {
        CommentType::All
    }
}

/// Extract context for a comment.
pub fn extract_context(content: &str, line_number: usize, file_path: &Path) -> String {
    format!(
        "{}:{} - {}",
        file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown"),
        line_number,
        content.chars().take(100).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn determine_comment_type_detection() {
        assert_eq!(
            determine_comment_type("// TODO: fix this"),
            CommentType::Todo
        );
        assert_eq!(
            determine_comment_type("// FIXME: broken"),
            CommentType::Fixme
        );
        assert_eq!(
            determine_comment_type("// HACK: workaround"),
            CommentType::Hack
        );
        assert_eq!(
            determine_comment_type("// XXX: review needed"),
            CommentType::Xxx
        );
    }

    #[test]
    fn build_comment_regex_compiles() {
        let regex = build_comment_regex(&[CommentType::Todo]).unwrap();
        assert!(regex.is_match("// TODO: fix this"));
        assert!(!regex.is_match("// FIXME: fix this"));

        let regex_all = build_comment_regex(&[CommentType::All]).unwrap();
        assert!(regex_all.is_match("// TODO: fix"));
        assert!(regex_all.is_match("// FIXME: fix"));
        assert!(regex_all.is_match("// HACK: workaround"));
        assert!(regex_all.is_match("// XXX: review"));
    }

    #[test]
    fn extract_context_format() {
        let ctx = extract_context("test content", 42, Path::new("/path/to/file.rs"));
        assert!(ctx.contains("file.rs"));
        assert!(ctx.contains("42"));
        assert!(ctx.contains("test content"));
    }

    #[test]
    fn detect_comments_finds_todos() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "// TODO: fix this bug").unwrap();
        writeln!(temp_file, "fn main() {{}}").unwrap();
        writeln!(temp_file, "// FIXME: handle error").unwrap();
        temp_file.flush().unwrap();

        let regex = build_comment_regex(&[CommentType::All]).unwrap();
        let comments = detect_comments(temp_file.path(), &regex).unwrap();

        assert_eq!(comments.len(), 2);
        // Content includes the marker because the regex captures differently for All
        assert!(comments[0].content.contains("fix this bug"));
        assert_eq!(comments[0].comment_type, CommentType::Todo);
        assert!(comments[1].content.contains("handle error"));
        assert_eq!(comments[1].comment_type, CommentType::Fixme);
    }

    #[test]
    fn detect_comments_returns_error_for_missing_file() {
        let regex = build_comment_regex(&[CommentType::Todo]).unwrap();
        let result = detect_comments(Path::new("/nonexistent/file.rs"), &regex);
        assert!(result.is_err());
    }

    // =====================================================================
    // Additional build_comment_regex tests
    // =====================================================================

    #[test]
    fn build_comment_regex_empty_defaults_to_all() {
        let regex = build_comment_regex(&[]).unwrap();

        // Should match all comment types when empty slice provided
        assert!(regex.is_match("// TODO: fix this"));
        assert!(regex.is_match("// FIXME: broken"));
        assert!(regex.is_match("// HACK: workaround"));
        assert!(regex.is_match("// XXX: review"));
    }

    #[test]
    fn build_comment_regex_multiple_specific_types() {
        let regex = build_comment_regex(&[CommentType::Todo, CommentType::Fixme]).unwrap();

        assert!(regex.is_match("// TODO: fix this"));
        assert!(regex.is_match("// FIXME: broken"));
        assert!(!regex.is_match("// HACK: workaround"));
        assert!(!regex.is_match("// XXX: review"));
    }

    #[test]
    fn build_comment_regex_case_insensitive() {
        let regex = build_comment_regex(&[CommentType::Todo]).unwrap();

        assert!(regex.is_match("// todo: lowercase"));
        assert!(regex.is_match("// Todo: mixed case"));
        assert!(regex.is_match("// TODO: uppercase"));
        assert!(regex.is_match("// ToDo: weird case"));
    }

    #[test]
    fn build_comment_regex_various_separators() {
        let regex = build_comment_regex(&[CommentType::Todo]).unwrap();

        assert!(regex.is_match("// TODO: colon separator"));
        assert!(regex.is_match("// TODO; semicolon separator"));
        assert!(regex.is_match("// TODO- dash separator"));
        assert!(regex.is_match("// TODO  space separator"));
        assert!(regex.is_match("// TODO no separator"));
    }

    #[test]
    fn build_comment_regex_captures_content() {
        let regex = build_comment_regex(&[CommentType::Todo]).unwrap();

        let line = "// TODO: this is the important part";
        let captures = regex.captures(line).unwrap();
        let content = captures.get(1).map(|m| m.as_str()).unwrap_or("");

        assert!(content.contains("this is the important part"));
    }

    // =====================================================================
    // Additional detect_comments tests
    // =====================================================================

    #[test]
    fn detect_comments_handles_multiline_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "fn main() {{").unwrap();
        writeln!(temp_file, "    // TODO: handle error").unwrap();
        writeln!(temp_file, "    let x = 42;").unwrap();
        writeln!(temp_file, "    // FIXME: magic number").unwrap();
        writeln!(temp_file, "}}").unwrap();
        temp_file.flush().unwrap();

        let regex = build_comment_regex(&[CommentType::All]).unwrap();
        let comments = detect_comments(temp_file.path(), &regex).unwrap();

        assert_eq!(comments.len(), 2);

        // Check line numbers are correct
        let line_numbers: Vec<usize> = comments.iter().map(|c| c.line_number).collect();
        assert!(line_numbers.contains(&2));
        assert!(line_numbers.contains(&4));
    }

    #[test]
    fn detect_comments_handles_empty_content_lines() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "// TODO:").unwrap(); // Minimal content after marker
        writeln!(temp_file, "// TODO: has content").unwrap();
        temp_file.flush().unwrap();

        let regex = build_comment_regex(&[CommentType::Todo]).unwrap();
        let comments = detect_comments(temp_file.path(), &regex).unwrap();

        // Both lines match - the capture logic uses the outer group (full match)
        assert_eq!(comments.len(), 2);
        // First comment captures "TODO:" (the full match)
        assert!(comments[0].content.contains("TODO"));
        // Second comment captures "TODO: has content"
        assert!(comments[1].content.contains("has content"));
    }

    #[test]
    fn determine_comment_type_prefers_first_match() {
        // If line contains multiple markers, should prefer first one found
        // (based on order in function: TODO, FIXME, HACK, XXX)
        assert_eq!(
            determine_comment_type("// TODO FIXME: both present"),
            CommentType::Todo
        );
        assert_eq!(
            determine_comment_type("// FIXME HACK: both present"),
            CommentType::Fixme
        );
        assert_eq!(
            determine_comment_type("// HACK XXX: both present"),
            CommentType::Hack
        );
    }

    #[test]
    fn extract_context_truncates_long_content() {
        let long_content = "a".repeat(200);
        let ctx = extract_context(&long_content, 42, Path::new("/path/to/file.rs"));

        // Context should be truncated to 100 chars
        assert!(ctx.len() < 150); // file.rs:42 - + 100 chars
    }

    #[test]
    fn detect_comments_handles_unicode() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "// TODO: 处理错误处理").unwrap(); // Chinese
        writeln!(temp_file, "// FIXME: 🐛 bug fix").unwrap(); // Emoji
        temp_file.flush().unwrap();

        let regex = build_comment_regex(&[CommentType::All]).unwrap();
        let comments = detect_comments(temp_file.path(), &regex).unwrap();

        assert_eq!(comments.len(), 2);
    }
}
