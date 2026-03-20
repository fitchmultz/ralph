//! Purpose: request parsing coverage for task command input normalization.
//!
//! Responsibilities:
//! - Validate positional-argument joining and whitespace trimming behavior.
//! - Verify error handling for missing or whitespace-only requests.
//! - Preserve special characters, multilingual text, newlines, tabs, and internal whitespace.
//!
//! Scope:
//! - `task_cmd::read_request_from_args_or_reader` behavior only.
//!
//! Usage:
//! - Uses `super::*;` to access the shared suite imports.
//!
//! Invariants/assumptions callers must respect:
//! - These tests intentionally use a `Cursor` reader instead of real stdin to avoid hangs in non-terminal CI environments.

use super::*;

#[test]
fn test_read_request_from_args_or_stdin_with_args() {
    let args = vec![
        "create".to_string(),
        "a".to_string(),
        "new".to_string(),
        "task".to_string(),
    ];

    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "create a new task");
}

#[test]
fn test_read_request_from_args_or_stdin_empty_args_fails() {
    let args: Vec<String> = vec![];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Missing request"));
}

#[test]
fn test_read_request_from_args_or_stdin_whitespace_args_fails() {
    let args: Vec<String> = vec!["   ".to_string(), "  ".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_err());
}

#[test]
fn test_read_request_from_args_or_stdin_trims_whitespace() {
    let args: Vec<String> = vec!["  hello  ".to_string(), "  world  ".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "hello     world");
}

#[test]
fn test_read_request_from_args_or_stdin_special_characters() {
    let args: Vec<String> = vec!["test: fix bug #123".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "test: fix bug #123");
}

#[test]
fn test_read_request_from_args_or_stdin_multilingual() {
    let args: Vec<String> = vec!["Hello 世界 🎉".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello 世界 🎉");
}

#[test]
fn test_read_request_single_arg() {
    let args = vec!["single".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "single");
}

#[test]
fn test_read_request_with_newlines() {
    let args = vec!["line1\nline2".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "line1\nline2");
}

#[test]
fn test_read_request_with_tabs() {
    let args = vec!["line1\tline2".to_string()];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "line1\tline2");
}

#[test]
fn test_read_request_preserves_internal_whitespace() {
    let args = vec![
        "word1   word2".to_string(),
        "word3".to_string(),
        "   word4   word5".to_string(),
    ];
    let reader = Cursor::new("");
    let result = task_cmd::read_request_from_args_or_reader(&args, true, reader);
    assert!(result.is_ok());
    assert!(result.unwrap().starts_with("word1   word2"));
}
