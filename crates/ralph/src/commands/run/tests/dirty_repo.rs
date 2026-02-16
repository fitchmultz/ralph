//! Dirty repository error detection tests (RQ-0801).

use crate::git::GitError;
use crate::runutil;

#[test]
fn dirty_repo_error_is_detected_in_error_chain() {
    // Create a DirtyRepo error wrapped in anyhow::Error
    let git_err = GitError::DirtyRepo {
        details: "uncommitted changes".to_string(),
    };
    let err = anyhow::anyhow!(git_err);

    // Verify it can be detected using the helper
    assert!(
        runutil::is_dirty_repo_error(&err),
        "Should detect DirtyRepo in error chain"
    );
}

#[test]
fn dirty_repo_error_with_context_is_detected() {
    // Create a DirtyRepo error with context wrapping
    let git_err = GitError::DirtyRepo {
        details: "uncommitted changes".to_string(),
    };
    let err = anyhow::anyhow!(git_err).context("running phase 1");

    // Verify it can still be detected through context layers
    assert!(
        runutil::is_dirty_repo_error(&err),
        "Should detect DirtyRepo through context layers"
    );
}

#[test]
fn non_dirty_repo_error_is_not_detected() {
    // Create a different GitError variant
    let git_err = GitError::NoUpstream;
    let err = anyhow::anyhow!(git_err);

    // Should NOT be detected as dirty repo
    assert!(
        !runutil::is_dirty_repo_error(&err),
        "Should NOT detect NoUpstream as DirtyRepo"
    );
}

#[test]
fn arbitrary_error_is_not_detected_as_dirty_repo() {
    // Create a generic anyhow error
    let err = anyhow::anyhow!("something went wrong");

    // Should NOT be detected as dirty repo
    assert!(
        !runutil::is_dirty_repo_error(&err),
        "Should NOT detect arbitrary error as DirtyRepo"
    );
}

#[test]
fn dirty_repo_error_deep_in_chain_is_detected() {
    // Create a deeply nested error chain
    let git_err = GitError::DirtyRepo {
        details: "uncommitted changes".to_string(),
    };
    let err = anyhow::anyhow!(git_err)
        .context("phase 2 failed")
        .context("task execution failed")
        .context("run loop iteration failed");

    // Should still detect DirtyRepo deep in the chain
    assert!(
        runutil::is_dirty_repo_error(&err),
        "Should detect DirtyRepo deep in error chain"
    );
}
