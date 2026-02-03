//! Test runner helpers for simulating CLI binaries.
//!
//! Responsibilities:
//! - Create executable fake runner binaries for tests that need runner output.
//! - Ensure deterministic setup of test runner scripts under a temp directory.
//!
//! Not handled here:
//! - Test orchestration or assertions (handled by individual tests).
//! - Cross-platform runner command semantics beyond basic executable creation.
//!
//! Invariants/assumptions:
//! - Callers provide valid script content compatible with the current platform shell.
//! - Unix permissions are required for executability on Unix platforms.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Create a fake runner binary in `dir/bin/<name>` with the provided script contents.
pub(crate) fn create_fake_runner(dir: &Path, name: &str, script: &str) -> Result<PathBuf> {
    let bin_dir = dir.join("bin");
    std::fs::create_dir(&bin_dir)?;
    let runner_path = bin_dir.join(name);
    std::fs::write(&runner_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&runner_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&runner_path, perms)?;
    }

    Ok(runner_path)
}
