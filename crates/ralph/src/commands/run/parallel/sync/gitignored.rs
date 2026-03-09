//! Gitignored allowlist syncing for worker workspaces.
//!
//! Responsibilities:
//! - Filter ignored repository paths down to the explicit worker allowlist.
//! - Copy safe ignored files such as `.env*` into worker workspaces.
//! - Avoid recursive self-copy when workspaces live under the repo root.
//!
//! Does NOT handle:
//! - `.ralph` runtime-tree traversal.
//! - Queue/done bookkeeping path seeding.
//!
//! Invariants:
//! - Directories and heavyweight cache/build trees are never copied.
//! - Only `.env` and `.env.*` basenames are allowlisted.

use crate::git;
use anyhow::{Context, Result};
use std::path::Path;

use super::common::sync_file_if_exists;

pub(super) fn should_sync_gitignored_entry(raw_git_entry: &str) -> bool {
    const NEVER_COPY_PREFIXES: &[&str] = &[
        "target/",
        "node_modules/",
        ".venv/",
        ".ralph/cache/",
        ".ralph/workspaces/",
        ".ralph/logs/",
        ".ralph/lock/",
        "__pycache__/",
        ".ruff_cache/",
        ".pytest_cache/",
        ".ty_cache/",
        ".git/",
    ];

    if raw_git_entry.is_empty() {
        return false;
    }

    let normalized = raw_git_entry.strip_prefix("./").unwrap_or(raw_git_entry);
    if normalized.ends_with('/') {
        return false;
    }

    for prefix in NEVER_COPY_PREFIXES {
        if normalized.starts_with(prefix) || normalized.contains(prefix) {
            return false;
        }
    }

    let basename = Path::new(normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");

    basename == ".env" || basename.starts_with(".env.")
}

pub(super) fn sync_gitignored(repo_root: &Path, workspace_path: &Path) -> Result<()> {
    let ignored = git::ignored_paths(repo_root)
        .with_context(|| format!("list ignored paths in {}", repo_root.display()))?;
    if ignored.is_empty() {
        return Ok(());
    }

    let workspace_rel = workspace_path.strip_prefix(repo_root).ok().map(|path| {
        path.to_string_lossy()
            .trim_end_matches(std::path::MAIN_SEPARATOR)
            .trim_end_matches('/')
            .to_string()
    });

    for rel in ignored {
        if !should_sync_gitignored_entry(&rel) {
            continue;
        }

        let rel_trimmed = rel.trim_end_matches('/');
        if rel_trimmed.is_empty() {
            continue;
        }

        if let Some(prefix) = &workspace_rel
            && (rel_trimmed == prefix
                || rel_trimmed.starts_with(&format!("{}/", prefix))
                || prefix.starts_with(&format!("{}/", rel_trimmed)))
        {
            continue;
        }

        let source = repo_root.join(rel_trimmed);
        let target = workspace_path.join(rel_trimmed);
        if !source.exists() {
            continue;
        }

        sync_file_if_exists(&source, &target)?;
    }

    Ok(())
}
