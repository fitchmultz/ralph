//! Path mapping utilities for parallel workspace operations.
//!
//! Responsibilities:
//! - Map resolved paths from the original repo into workspace clones.
//! - Provide path traversal protection for security.
//!
//! Not handled here:
//! - File I/O operations (see `sync.rs` for copying).
//! - Config resolution (see `crate::config`).
//!
//! Invariants/assumptions:
//! - Resolved paths are expected to be under the repo root.
//! - Workspace repo root is a separate directory (clone) of the original repo.
//! - Paths containing `..` components are rejected to prevent directory traversal.

use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};

/// Map a resolved path from the original repo into the workspace clone.
///
/// The resolved path is expected to be under `repo_root`. This strips the
/// repo_root prefix to get a repo-relative path, validates it doesn't contain
/// `..` components (path traversal protection), and joins it onto the workspace root.
///
/// # Arguments
/// * `repo_root` - The root directory of the original repository
/// * `workspace_repo_root` - The root directory of the workspace clone
/// * `resolved_path` - The absolute path to map (must be under repo_root)
/// * `label` - A label for error messages (e.g., "queue", "done")
///
/// # Errors
/// Returns an error if:
/// - The resolved path is not under the repo root
/// - The repo-relative path contains `..` components
pub(crate) fn map_resolved_path_into_workspace(
    repo_root: &Path,
    workspace_repo_root: &Path,
    resolved_path: &Path,
    label: &str,
) -> Result<PathBuf> {
    // Get the repo-relative path
    let relative = resolved_path.strip_prefix(repo_root).with_context(|| {
        format!(
            "{} path {} is not under repo root {}",
            label,
            resolved_path.display(),
            repo_root.display()
        )
    })?;

    // Security: reject paths containing ".." components
    for component in relative.components() {
        if component == Component::ParentDir {
            bail!(
                "{} path contains '..' component: {}",
                label,
                relative.display()
            );
        }
    }

    Ok(workspace_repo_root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_resolved_path_into_workspace_rejects_traversal() {
        let repo_root = PathBuf::from("/repo");
        let workspace_root = PathBuf::from("/workspace");

        // Path containing .. should be rejected
        let bad_path = PathBuf::from("/repo/../etc/passwd");
        let result =
            map_resolved_path_into_workspace(&repo_root, &workspace_root, &bad_path, "test");
        assert!(result.is_err(), "Path with .. should be rejected");

        // Path outside repo root should be rejected
        let outside_path = PathBuf::from("/other/file.json");
        let result =
            map_resolved_path_into_workspace(&repo_root, &workspace_root, &outside_path, "test");
        assert!(result.is_err(), "Path outside repo root should be rejected");
    }

    #[test]
    fn map_resolved_path_into_workspace_accepts_valid_path() {
        let repo_root = PathBuf::from("/repo");
        let workspace_root = PathBuf::from("/workspace");
        let resolved_path = PathBuf::from("/repo/.ralph/queue.json");

        let result =
            map_resolved_path_into_workspace(&repo_root, &workspace_root, &resolved_path, "queue");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/workspace/.ralph/queue.json")
        );
    }

    #[test]
    fn map_resolved_path_into_workspace_accepts_nested_path() {
        let repo_root = PathBuf::from("/repo");
        let workspace_root = PathBuf::from("/workspace");
        let resolved_path = PathBuf::from("/repo/queue/active.json");

        let result =
            map_resolved_path_into_workspace(&repo_root, &workspace_root, &resolved_path, "queue");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/workspace/queue/active.json")
        );
    }
}
