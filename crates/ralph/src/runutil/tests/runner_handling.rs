//! Runner-handling tests for runutil.
//!
//! Purpose:
//! - Runner-handling tests for runutil.
//!
//! Responsibilities:
//! - Exercise interrupt, timeout, revert, and output-stream handling with stub backends.
//! - Verify abort reasons and safeguard file behavior for handled runner failures.
//!
//! Non-scope:
//! - Revert prompt parsing details.
//! - Queue validation error classification.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Stub backends avoid real runner binaries.
//! - Temp git repos isolate file mutation and revert assertions.

use super::fixtures::{
    CaptureBackend, InterruptBackend, NonZeroBackend, TimeoutBackend, base_invocation,
    base_messages, commit_file, init_git_repo,
};
use crate::contracts::GitRevertMode;
use crate::git;
use crate::runutil::{
    RevertDecision, RunAbortReason, abort_reason, run_prompt_with_handling_backend,
};
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn run_prompt_interrupt_returns_abort_reason() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let invocation = base_invocation(dir.path());
    let messages = base_messages("interrupt_test");

    let mut backend = InterruptBackend;
    let err = run_prompt_with_handling_backend(invocation, messages, &mut backend).unwrap_err();
    assert_eq!(abort_reason(&err), Some(RunAbortReason::Interrupted));
}

#[test]
fn run_prompt_user_revert_returns_abort_reason() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let mut invocation = base_invocation(dir.path());
    invocation.failure.revert_on_error = true;
    invocation.failure.git_revert_mode = GitRevertMode::Ask;
    invocation.failure.revert_prompt = Some(Arc::new(|_context| Ok(RevertDecision::Revert)));

    let mut backend = NonZeroBackend;
    let err =
        run_prompt_with_handling_backend(invocation, base_messages("non_zero_test"), &mut backend)
            .unwrap_err();
    assert_eq!(abort_reason(&err), Some(RunAbortReason::UserRevert));

    let reverted = fs::read_to_string(&file_path).expect("read file after revert");
    assert_eq!(reverted, "original");
}

#[test]
fn timeout_applies_git_revert_mode_and_saves_safeguard_dump_when_stdout_is_available() {
    let dir = TempDir::new().expect("temp dir");
    init_git_repo(&dir);
    commit_file(&dir, "file.txt", "original", "initial");

    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "modified").expect("modify file");

    let mut invocation = base_invocation(dir.path());
    invocation.settings.timeout = Some(Duration::from_millis(10));
    invocation.failure.revert_on_error = true;
    invocation.failure.git_revert_mode = GitRevertMode::Enabled;

    let mut backend = TimeoutBackend {
        emitted: "hello from runner before timeout\n".to_string(),
    };

    let err =
        run_prompt_with_handling_backend(invocation, base_messages("timeout_test"), &mut backend)
            .unwrap_err();
    let msg = format!("{err:#}");

    assert!(msg.contains("timed out"));
    assert!(msg.contains("Uncommitted changes were reverted."));
    assert!(msg.contains("redacted output saved to"));

    let reverted = fs::read_to_string(&file_path).expect("read file after revert");
    assert_eq!(reverted, "original");

    let status = git::status_porcelain(dir.path()).expect("git status --porcelain -z");
    assert!(
        status.trim().is_empty(),
        "expected clean repo after timeout revert"
    );

    let marker = "redacted output saved to ";
    let start = msg.find(marker).map(|idx| idx + marker.len()).unwrap();
    let tail = &msg[start..];
    let end = tail.find(')').unwrap_or(tail.len());
    let path_str = tail[..end].trim();

    let dump = std::path::Path::new(path_str);
    assert!(
        dump.is_file(),
        "expected safeguard dump to exist: {path_str}"
    );
    let dump_contents = fs::read_to_string(dump).expect("read safeguard dump");
    assert!(dump_contents.contains("hello from runner before timeout"));
}

#[test]
fn run_prompt_passes_output_stream_to_backend() {
    let dir = TempDir::new().expect("temp dir");
    let mut invocation = base_invocation(dir.path());
    invocation.settings.output_stream = crate::runner::OutputStream::HandlerOnly;

    let mut backend = CaptureBackend {
        seen_output_stream: None,
    };

    let _ = run_prompt_with_handling_backend(invocation, base_messages("capture"), &mut backend);
    assert_eq!(
        backend.seen_output_stream,
        Some(crate::runner::OutputStream::HandlerOnly)
    );
}
