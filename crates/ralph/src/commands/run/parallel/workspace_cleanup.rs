//! Shared workspace cleanup helpers for parallel execution.
//!
//! Purpose:
//! - Shared workspace cleanup helpers for parallel execution.
//!
//! Responsibilities:
//! - Provide a single best-effort workspace removal function used by orchestration,
//!   cleanup guard, and state initialization.
//!
//! Not handled here:
//! - Worker spawning or lifecycle (see `super::worker`).
//! - State persistence (see `super::state`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Cleanup is always best-effort: failures are logged as warnings and do not propagate.
//! - Uses `git::remove_workspace` with force=true to handle dirty workspaces.

use std::path::Path;

use crate::git;

/// Remove a workspace with best-effort semantics.
///
/// Logs success or warning on failure. Never returns an error.
///
/// # Arguments
/// * `workspace_root` - The root directory containing all parallel workspaces.
/// * `workspace` - The workspace specification (path and branch).
/// * `reason` - Description of why cleanup is happening (for logging).
pub(super) fn remove_workspace_best_effort(
    workspace_root: &Path,
    workspace: &git::WorkspaceSpec,
    reason: &str,
) {
    if let Err(err) = git::remove_workspace(workspace_root, workspace, true) {
        log::warn!(
            "Failed to remove workspace for {} during {}: {err:#}",
            workspace.path.display(),
            reason
        );
    } else {
        log::info!(
            "Deleted workspace for {} ({})",
            workspace.path.display(),
            reason
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn removes_existing_workspace() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        git_test::git_run(
            temp.path(),
            &["remote", "add", "origin", "https://example.com/repo.git"],
        )?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let workspace_root = temp.path().join("workspaces");

        let spec = git::create_workspace_at(temp.path(), &workspace_root, "RQ-TEST", &base_branch)?;
        assert!(spec.path.exists());

        remove_workspace_best_effort(&workspace_root, &spec, "test cleanup");

        assert!(!spec.path.exists());
        Ok(())
    }

    #[test]
    fn handles_nonexistent_workspace_gracefully() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).unwrap();

        let spec = git::WorkspaceSpec {
            path: workspace_root.join("RQ-NONEXISTENT"),
            branch: "main".to_string(),
        };

        // Should not panic or log errors for nonexistent workspace
        remove_workspace_best_effort(&workspace_root, &spec, "test cleanup");
    }
}
