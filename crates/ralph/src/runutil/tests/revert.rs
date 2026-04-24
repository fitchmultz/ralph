//! Revert and prompt parsing tests for runutil.
//!
//! Purpose:
//! - Revert and prompt parsing tests for runutil.
//!
//! Responsibilities:
//! - Validate revert prompt parsing and rendered prompt ordering.
//! - Verify git revert mode outcomes under deterministic prompt handlers.
//!
//! Non-scope:
//! - Runner backend error handling.
//! - Queue validation error classification.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Tests operate on isolated temp repos.
//! - Prompt rendering is asserted from captured in-memory output.

use super::fixtures::{commit_file, init_git_repo};
use crate::contracts::GitRevertMode;
use crate::runutil::{
    RevertDecision, RevertOutcome, RevertPromptContext, RevertPromptHandler, RevertSource,
    apply_git_revert_mode, apply_git_revert_mode_with_context, parse_revert_response,
    prompt_revert_choice_with_io,
};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

fn decide_ask(stdin_is_terminal: bool, input: Option<&str>) -> RevertDecision {
    if !stdin_is_terminal {
        return RevertDecision::Keep;
    }
    parse_revert_response(input.unwrap_or(""), false)
}

#[test]
fn ask_mode_defaults_to_keep_when_non_interactive() {
    assert_eq!(decide_ask(false, Some("1")), RevertDecision::Keep);
}

#[test]
fn parse_revert_response_accepts_expected_inputs() {
    assert_eq!(parse_revert_response("", false), RevertDecision::Keep);
    assert_eq!(parse_revert_response("1", false), RevertDecision::Keep);
    assert_eq!(parse_revert_response("keep", false), RevertDecision::Keep);
    assert_eq!(parse_revert_response("2", false), RevertDecision::Revert);
    assert_eq!(parse_revert_response("r", false), RevertDecision::Revert);
    assert_eq!(
        parse_revert_response("revert", false),
        RevertDecision::Revert
    );
    assert_eq!(
        parse_revert_response("3", false),
        RevertDecision::Continue {
            message: String::new()
        }
    );
    assert_eq!(
        parse_revert_response("answer that", false),
        RevertDecision::Continue {
            message: "answer that".to_string()
        }
    );
}

#[test]
fn parse_revert_response_allows_proceed_when_enabled() {
    assert_eq!(parse_revert_response("4", true), RevertDecision::Proceed);
    assert_eq!(
        parse_revert_response("4", false),
        RevertDecision::Continue {
            message: "4".to_string()
        }
    );
}

fn prompt_with_preface(input: &str) -> (RevertDecision, String) {
    let context = RevertPromptContext::new("Scan validation failure", false).with_preface(
        "Scan validation failed after run.\n(raw stdout saved to /tmp/output.txt)\nDetails",
    );
    let mut reader = std::io::Cursor::new(input.as_bytes());
    let mut output = Vec::new();
    let decision = prompt_revert_choice_with_io(&context, &mut reader, &mut output)
        .expect("prompt with preface");
    let rendered = String::from_utf8(output).expect("output utf8");
    (decision, rendered)
}

#[test]
fn prompt_revert_choice_writes_preface_before_prompt_for_keep() {
    let (decision, output) = prompt_with_preface("1\n");
    assert_eq!(decision, RevertDecision::Keep);
    let preface_idx = output.find("Scan validation failed after run.").unwrap();
    let prompt_idx = output.find("Scan validation failure: action?").unwrap();
    assert!(
        preface_idx < prompt_idx,
        "expected preface before prompt, got: {output:?}"
    );
}

#[test]
fn prompt_revert_choice_writes_preface_before_prompt_for_revert() {
    let (decision, output) = prompt_with_preface("2\n");
    assert_eq!(decision, RevertDecision::Revert);
    let preface_idx = output.find("Scan validation failed after run.").unwrap();
    let prompt_idx = output.find("Scan validation failure: action?").unwrap();
    assert!(
        preface_idx < prompt_idx,
        "expected preface before prompt, got: {output:?}"
    );
}

#[test]
fn apply_git_revert_mode_uses_prompt_handler_keep() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| Ok(RevertDecision::Keep));
    let outcome = apply_git_revert_mode(
        dir.path(),
        GitRevertMode::Ask,
        "test prompt",
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Skipped {
            reason: "user chose to keep changes".to_string()
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "modified");
}

#[test]
fn apply_git_revert_mode_uses_prompt_handler_revert() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| Ok(RevertDecision::Revert));
    let outcome = apply_git_revert_mode(
        dir.path(),
        GitRevertMode::Ask,
        "test prompt",
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Reverted {
            source: RevertSource::User
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "original");
}

#[test]
fn apply_git_revert_mode_uses_prompt_handler_continue() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| {
        Ok(RevertDecision::Continue {
            message: "keep going".to_string(),
        })
    });
    let outcome = apply_git_revert_mode(
        dir.path(),
        GitRevertMode::Ask,
        "test prompt",
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Continue {
            message: "keep going".to_string()
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "modified");
}

#[test]
fn apply_git_revert_mode_allows_proceed_when_enabled() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let handler: RevertPromptHandler = Arc::new(|_context| Ok(RevertDecision::Proceed));
    let outcome = apply_git_revert_mode_with_context(
        dir.path(),
        GitRevertMode::Ask,
        RevertPromptContext::new("test prompt", true),
        Some(&handler),
    )
    .expect("apply revert mode");

    assert_eq!(
        outcome,
        RevertOutcome::Proceed {
            reason: "user chose to proceed".to_string()
        }
    );
    let contents = fs::read_to_string(&file_path).expect("read file");
    assert_eq!(contents, "modified");
}
