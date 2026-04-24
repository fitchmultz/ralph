//! Path mapping utilities for parallel workspace operations.
//!
//! Purpose:
//! - Path mapping utilities for parallel workspace operations.
//!
//! Responsibilities:
//! - Map resolved paths from the original repo into workspace clones.
//! - Provide path traversal protection for security.
//!
//! Not handled here:
//! - File I/O operations (see `sync.rs` for copying).
//! - Config resolution (see `crate::config`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Resolved paths are expected to be under the repo root.
//! - Workspace repo root is a separate directory (clone) of the original repo.
//! - Paths containing `..` components are rejected to prevent directory traversal.

use anyhow::{Context, Result, bail};
use std::fs;
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
    if contains_parent_dir(resolved_path) {
        bail!(
            "{} path contains '..' component: {}",
            label,
            resolved_path.display()
        );
    }

    // Fast path: lexical prefix match for normal absolute paths.
    // Fallback canonicalizes when repo and target paths are equivalent but represented
    // differently (for example macOS /var vs /private path aliases).
    let relative = match resolved_path.strip_prefix(repo_root) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => {
            let canonical_repo_root = canonicalize_allow_missing_tail(repo_root)
                .with_context(|| format!("canonicalize repo root {}", repo_root.display()))?;
            let canonical_resolved_path = canonicalize_allow_missing_tail(resolved_path)
                .with_context(|| {
                    format!("canonicalize {} path {}", label, resolved_path.display())
                })?;

            canonical_resolved_path
                .strip_prefix(&canonical_repo_root)
                .with_context(|| {
                    format!(
                        "{} path {} is not under repo root {}",
                        label,
                        resolved_path.display(),
                        repo_root.display()
                    )
                })?
                .to_path_buf()
        }
    };

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

    Ok(workspace_repo_root.join(&relative))
}

fn contains_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|component| component == Component::ParentDir)
}

fn canonicalize_allow_missing_tail(path: &Path) -> Result<PathBuf> {
    let mut missing_tail = Vec::new();
    let mut cursor = path;

    while !cursor.exists() {
        let Some(name) = cursor.file_name() else {
            break;
        };
        missing_tail.push(name.to_os_string());
        let Some(parent) = cursor.parent() else {
            break;
        };
        cursor = parent;
    }

    let mut canonical = fs::canonicalize(cursor)
        .with_context(|| format!("canonicalize existing path {}", cursor.display()))?;
    for component in missing_tail.iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

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

    #[cfg(unix)]
    #[test]
    fn map_resolved_path_into_workspace_accepts_symlinked_repo_aliases() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path().join("repo-real");
        let repo_alias = temp.path().join("repo-alias");
        let workspace_root = temp.path().join("workspace");

        std::fs::create_dir_all(repo_root.join(".ralph"))?;
        symlink(&repo_root, &repo_alias)?;
        let resolved_path = repo_alias.join(".ralph/queue.json");
        std::fs::write(&resolved_path, "{}")?;

        let mapped =
            map_resolved_path_into_workspace(&repo_root, &workspace_root, &resolved_path, "queue")?;
        assert_eq!(mapped, workspace_root.join(".ralph/queue.json"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn map_resolved_path_into_workspace_handles_missing_tail_with_repo_alias() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path().join("repo-real");
        let repo_alias = temp.path().join("repo-alias");
        let workspace_root = temp.path().join("workspace");

        std::fs::create_dir_all(repo_root.join(".ralph"))?;
        symlink(&repo_root, &repo_alias)?;
        let resolved_path = repo_alias.join(".ralph/cache/missing-done.json");

        let mapped =
            map_resolved_path_into_workspace(&repo_root, &workspace_root, &resolved_path, "done")?;
        assert_eq!(
            mapped,
            workspace_root.join(".ralph/cache/missing-done.json")
        );
        Ok(())
    }
}
