//! Comment detection for the watch command.
//!
//! Purpose:
//! - Comment detection for the watch command.
//!
//! Responsibilities:
//! - Build regex patterns for detecting TODO/FIXME/HACK/XXX comments.
//! - Detect comments in source files.
//! - Determine comment type from line content.
//! - Extract context for detected comments.
//!
//! Not handled here:
//! - File watching (see `event_loop/mod.rs`).
//! - Task creation from comments (see `tasks.rs`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Comment regex is case-insensitive.
//! - Comment extraction uses stable named captures for marker and content.
//! - Context includes filename, line number, and truncated content.

use crate::commands::watch::types::{CommentType, DetectedComment};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;

/// Build regex for detecting comments based on comment types.
pub fn build_comment_regex(comment_types: &[CommentType]) -> Result<Regex> {
    let markers = selected_comment_markers(comment_types);
    let combined = markers.join("|");
    let regex = Regex::new(&format!(
        r"(?i)(?P<marker>{combined})(?:\s*[:;-]\s*|\s+)(?P<content>.+)$"
    ))
    .context("Failed to compile comment detection regex")?;

    Ok(regex)
}

fn selected_comment_markers(comment_types: &[CommentType]) -> Vec<&'static str> {
    let has_all = comment_types.contains(&CommentType::All) || comment_types.is_empty();
    let mut markers = Vec::new();

    if has_all || comment_types.contains(&CommentType::Todo) {
        markers.push("TODO");
    }
    if has_all || comment_types.contains(&CommentType::Fixme) {
        markers.push("FIXME");
    }
    if has_all || comment_types.contains(&CommentType::Hack) {
        markers.push("HACK");
    }
    if has_all || comment_types.contains(&CommentType::Xxx) {
        markers.push("XXX");
    }

    markers
}

/// Detect comments in a file.
pub fn detect_comments(file_path: &Path, regex: &Regex) -> Result<Vec<DetectedComment>> {
    let content = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let mut comments = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(captures) = regex.captures(line) {
            let content = captures
                .name("content")
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();

            if content.is_empty() {
                continue;
            }

            let comment_type = captures
                .name("marker")
                .map(|m| comment_type_from_marker(m.as_str()))
                .unwrap_or_else(|| determine_comment_type(line));
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

fn comment_type_from_marker(marker: &str) -> CommentType {
    match marker.trim().to_ascii_uppercase().as_str() {
        "TODO" => CommentType::Todo,
        "FIXME" => CommentType::Fixme,
        "HACK" => CommentType::Hack,
        "XXX" => CommentType::Xxx,
        _ => CommentType::All,
    }
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
        assert_eq!(comments[0].content, "fix this bug");
        assert_eq!(comments[0].comment_type, CommentType::Todo);
        assert_eq!(comments[1].content, "handle error");
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
        let content = captures.name("content").map(|m| m.as_str()).unwrap_or("");

        assert_eq!(content, "this is the important part");
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

        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].content, "has content");
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
        assert_eq!(comments[0].content, "处理错误处理");
        assert_eq!(comments[1].content, "🐛 bug fix");
    }

    #[test]
    fn detect_comments_extracts_identical_content_across_modes() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "// TODO: normalize watch output").unwrap();
        temp_file.flush().unwrap();

        let all_regex = build_comment_regex(&[CommentType::All]).unwrap();
        let todo_regex = build_comment_regex(&[CommentType::Todo]).unwrap();

        let all_comments = detect_comments(temp_file.path(), &all_regex).unwrap();
        let todo_comments = detect_comments(temp_file.path(), &todo_regex).unwrap();

        assert_eq!(all_comments.len(), 1);
        assert_eq!(todo_comments.len(), 1);
        assert_eq!(all_comments[0].content, todo_comments[0].content);
        assert_eq!(all_comments[0].comment_type, todo_comments[0].comment_type);
    }
}
