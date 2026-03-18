//! Failure-path queue issue publish tests.
//!
//! Responsibilities:
//! - Verify user-facing errors for missing tasks and missing `gh` binaries.
//! - Verify unauthenticated `gh` flows surface the expected guidance.
//! - Keep failure coverage separate from success and preview behavior.
//!
//! Not handled here:
//! - Execute-mode success cases.
//! - Publish-many happy-path assertions.
//! - Fake `gh` script implementation details.
//!
//! Invariants/assumptions:
//! - Test function names remain stable.
//! - Error assertions stay tolerant to the expected message variants.
//! - Unix-specific auth failure coverage remains gated.

use super::{base_issue_publish_args, resolved_for_dir, run_issue_publish, write_queue};
use anyhow::Result;
use tempfile::TempDir;

#[test]
fn queue_issue_publish_fails_when_task_not_found() {
    let dir = TempDir::new().expect("temp dir");
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path).expect("write queue");

    let args = base_issue_publish_args("RQ-9999");

    let err = run_issue_publish(&resolved, true, args).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("RQ-9999"),
        "unexpected error: {msg}"
    );
}

#[test]
fn queue_issue_publish_fails_when_gh_missing() -> Result<()> {
    use crate::testsupport::path::with_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let args = base_issue_publish_args("RQ-0001");

    let err =
        with_path("", || run_issue_publish(&resolved, true, args)).expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("GitHub CLI (`gh`) not found on PATH"),
        "unexpected error: {msg}"
    );

    Ok(())
}

#[cfg(unix)]
#[test]
fn queue_issue_publish_fails_when_gh_unauthenticated() -> Result<()> {
    use super::create_fake_gh_for_issue_publish;
    use crate::testsupport::path::with_prepend_path;

    let dir = TempDir::new()?;
    let resolved = resolved_for_dir(&dir);
    write_queue(&resolved.queue_path)?;

    let bin_dir = create_fake_gh_for_issue_publish(
        &dir,
        "RQ-0001",
        "https://github.com/org/repo/issues/123",
        false,
    );

    let args = base_issue_publish_args("RQ-0001");
    let err = with_prepend_path(&bin_dir, || run_issue_publish(&resolved, true, args))
        .expect_err("expected error");
    let msg = err.to_string();
    assert!(
        msg.contains("not authenticated") && msg.contains("gh auth login"),
        "unexpected error: {msg}"
    );

    Ok(())
}
