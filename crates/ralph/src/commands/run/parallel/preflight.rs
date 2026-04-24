//! Workspace-root gitignore preflight for parallel runs.
//!
//! Purpose:
//! - Workspace-root gitignore preflight for parallel runs.
//!
//! Responsibilities:
//! - Ensure `parallel.workspace_root` is ignored by git when it lives inside the repo, so clone workspaces do not dirty the working tree.
//!
//! Not handled here:
//! - Run-loop bootstrap or queue validation (see `orchestration/preflight.rs`).
//! - Creating workspace directories.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `repo_root` and `workspace_root` are absolute or normalized paths Ralph already resolved.

use crate::git;
use anyhow::{Context, Result, bail};
use std::path::Path;

pub(crate) fn preflight_parallel_workspace_root_is_gitignored(
    repo_root: &Path,
    workspace_root: &Path,
) -> Result<()> {
    // Only enforce when workspace_root is inside the repo.
    let Ok(rel) = workspace_root.strip_prefix(repo_root) else {
        return Ok(());
    };

    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let rel_trimmed = rel_str.trim_matches('/');

    // If workspace_root == repo_root, that effectively asks to ignore the whole repo (nonsense).
    if rel_trimmed.is_empty() {
        bail!(
            "Parallel preflight: parallel.workspace_root resolves to the repo root ({}). Refusing to run.",
            repo_root.display()
        );
    }

    // Check ignore rules without creating the directory:
    let dir_candidate = rel_trimmed.to_string();
    let dummy_candidate = format!("{}/__ralph_ignore_probe__", rel_trimmed);

    let ignored_dir = git::is_path_ignored(repo_root, &dir_candidate)
        .with_context(|| format!("Parallel preflight: check-ignore {}", dir_candidate))?;
    let ignored_dummy = git::is_path_ignored(repo_root, &dummy_candidate)
        .with_context(|| format!("Parallel preflight: check-ignore {}", dummy_candidate))?;

    if ignored_dir || ignored_dummy {
        return Ok(());
    }

    let ignore_rule = format!("{}/", rel_trimmed.trim_end_matches('/'));
    bail!(
        "Parallel preflight: parallel.workspace_root resolves inside the repo but is not gitignored.\n\
workspace_root: {}\n\
repo_root: {}\n\
\n\
Ralph will create clone workspaces under this directory, which would leave untracked files and make the repo appear dirty.\n\
\n\
Fix options:\n\
1) Recommended: set parallel.workspace_root to an absolute path OUTSIDE the repo (or remove it to use the default outside-repo location).\n\
2) If you intentionally keep workspaces inside the repo, ignore it:\n\
   - Shared (tracked): add `{}` to `.gitignore` and commit it\n\
   - Local-only: add `{}` to `.git/info/exclude`\n",
        workspace_root.display(),
        repo_root.display(),
        ignore_rule,
        ignore_rule
    );
}
