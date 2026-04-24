//! Shared sync filesystem helpers.
//!
//! Purpose:
//! - Shared sync filesystem helpers.
//!
//! Responsibilities:
//! - Provide small, reusable file-copy helpers shared by sync submodules.
//!
//! Non-scope:
//! - Runtime-tree traversal policy.
//! - Gitignored allowlist decisions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Missing source files are treated as a no-op.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub(super) fn sync_file_if_exists(source: &Path, target: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create workspace dir {}", parent.display()))?;
    }
    fs::copy(source, target)
        .with_context(|| format!("sync {} to {}", source.display(), target.display()))?;
    Ok(())
}
