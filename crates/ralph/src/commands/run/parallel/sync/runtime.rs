//! `.ralph` runtime tree synchronization.
//!
//! Responsibilities:
//! - Copy repo-local `.ralph` runtime content into worker workspaces.
//! - Exclude queue/done bookkeeping files that are seeded from resolved paths.
//! - Skip coordinator-only runtime directories such as cache, logs, locks, and workspaces.
//!
//! Does NOT handle:
//! - Gitignored non-`.ralph` allowlist syncing.
//! - Bookkeeping path mapping into workspace roots.
//!
//! Invariants:
//! - Resolved queue/done files are always synced explicitly outside this module.
//! - Top-level runtime skip policy stays centralized in `should_skip_ralph_runtime_path`.

use crate::config;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use super::common::sync_file_if_exists;

pub(super) fn sync_ralph_runtime_tree(
    resolved: &config::Resolved,
    source_root: &Path,
    target_root: &Path,
) -> Result<()> {
    if !source_root.is_dir() {
        return Ok(());
    }

    sync_ralph_runtime_tree_recursive(resolved, source_root, source_root, target_root)
}

fn sync_ralph_runtime_tree_recursive(
    resolved: &config::Resolved,
    source_root: &Path,
    current_source_dir: &Path,
    target_root: &Path,
) -> Result<()> {
    for entry in fs::read_dir(current_source_dir)
        .with_context(|| format!("read ralph dir {}", current_source_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("read ralph entry in {}", current_source_dir.display()))?;
        let source_path = entry.path();

        if should_skip_ralph_runtime_path(resolved, source_root, &source_path) {
            continue;
        }

        let rel_path = source_path.strip_prefix(source_root).with_context(|| {
            format!(
                "derive relative path from {} to {}",
                source_root.display(),
                source_path.display()
            )
        })?;
        let target_path = target_root.join(rel_path);

        let file_type = entry
            .file_type()
            .with_context(|| format!("read type for {}", source_path.display()))?;
        if file_type.is_dir() {
            fs::create_dir_all(&target_path)
                .with_context(|| format!("create workspace dir {}", target_path.display()))?;
            sync_ralph_runtime_tree_recursive(resolved, source_root, &source_path, target_root)?;
            continue;
        }

        sync_file_if_exists(&source_path, &target_path)?;
    }

    Ok(())
}

fn should_skip_ralph_runtime_path(
    resolved: &config::Resolved,
    source_root: &Path,
    source_path: &Path,
) -> bool {
    const NEVER_COPY_RALPH_DIRS: &[&str] = &["cache", "workspaces", "logs", "lock"];

    let Ok(rel_path) = source_path.strip_prefix(source_root) else {
        return true;
    };

    if rel_path.components().count() == 1
        && let Some(name) = rel_path.file_name().and_then(|name| name.to_str())
        && matches!(
            name,
            "queue.json" | "queue.jsonc" | "done.json" | "done.jsonc"
        )
    {
        return true;
    }

    if source_path == resolved.queue_path || source_path == resolved.done_path {
        return true;
    }

    rel_path
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|component| NEVER_COPY_RALPH_DIRS.contains(&component))
}
