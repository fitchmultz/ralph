//! Safety tests for git cleanup behavior in Ralph.

use anyhow::{Context, Result};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn revert_uncommitted_preserves_untracked_env_files() -> Result<()> {
    let dir = TempDir::new()?;
    let root = dir.path();

    // 1. Init git repo
    Command::new("git")
        .current_dir(root)
        .args(["init", "--quiet"])
        .output()
        .context("git init")?;

    // 2. Commit a base file so we have a HEAD
    fs::write(root.join("README.md"), "# Test")?;
    Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .status()?;
    Command::new("git")
        .current_dir(root)
        .args(["commit", "-m", "init"])
        .status()?;

    // 3. Create untracked files
    let env_file = root.join(".env");
    let env_local = root.join(".env.local");
    let garbage = root.join("garbage.txt");

    fs::write(&env_file, "SECRET=123")?;
    fs::write(&env_local, "SECRET=456")?;
    fs::write(&garbage, "trash")?;

    // 4. Run revert_uncommitted
    ralph::git::revert_uncommitted(root)?;

    // 5. Verify assertions
    assert!(env_file.exists(), ".env should be preserved");
    assert!(env_local.exists(), ".env.local should be preserved");
    assert!(!garbage.exists(), "garbage.txt should be removed");

    Ok(())
}
