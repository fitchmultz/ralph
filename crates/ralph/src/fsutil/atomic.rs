//! Purpose: Atomic file-write helpers for local filesystem persistence.
//!
//! Responsibilities:
//! - Persist file contents atomically through same-directory temp files.
//! - Flush and sync temp files before replacement.
//! - Best-effort sync parent directories after successful replacement.
//!
//! Scope:
//! - Local atomic write orchestration only; temp cleanup policy and safeguard dumps live elsewhere.
//!
//! Usage:
//! - Used by queue, config, session, undo, migration, and runtime persistence paths.
//!
//! Invariants/Assumptions:
//! - Atomic writes require a parent directory.
//! - Temp files are created in the destination directory so `persist` remains local.
//! - Directory syncing is best-effort and Unix-only.
//! - Persist failures must drop the temp file handle to avoid leaving temp files behind.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    log::debug!("atomic write: {}", path.display());
    let dir = path
        .parent()
        .context("atomic write requires a parent directory")?;
    fs::create_dir_all(dir).with_context(|| format!("create directory {}", dir.display()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("create temp file in {}", dir.display()))?;
    tmp.write_all(contents).context("write temp file")?;
    tmp.flush().context("flush temp file")?;
    tmp.as_file().sync_all().context("sync temp file")?;

    match tmp.persist(path) {
        Ok(_) => {
            // Atomic replacement succeeded; no additional cleanup is needed here.
        }
        Err(err) => {
            // Explicitly drop the temp file to ensure cleanup on persist failure.
            // PersistError contains both the error and the NamedTempFile handle;
            // we must extract and drop the file handle to prevent temp file leaks.
            let _temp_file = err.file;
            drop(_temp_file);
            return Err(err.error).with_context(|| format!("persist {}", path.display()));
        }
    }

    sync_dir_best_effort(dir);
    Ok(())
}

pub(crate) fn sync_dir_best_effort(dir: &Path) {
    #[cfg(unix)]
    {
        log::debug!("syncing directory: {}", dir.display());
        if let Ok(file) = fs::File::open(dir) {
            let _ = file.sync_all();
        }
    }

    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}
