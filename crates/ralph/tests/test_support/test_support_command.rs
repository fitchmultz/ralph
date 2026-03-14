//! Command and repo helpers for integration tests.
//!
//! Responsibilities:
//! - Resolve the built `ralph` binary and run isolated subprocesses for tests.
//! - Initialize disposable git repos and executable fixtures.
//! - Provide scoped PATH mutation utilities for fake toolchains.
//!
//! Does not handle:
//! - Queue fixtures, config mutation, or snapshot normalization.
//!
//! Invariants/assumptions callers must respect:
//! - Callers that need cross-test PATH isolation must hold `env_lock()` while using `with_prepend_path`.
//! - Executable fixture helpers mark scripts executable only on Unix hosts.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

pub fn ralph_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    let candidate = profile_dir.join(bin_name);
    if candidate.exists() {
        return candidate;
    }

    panic!(
        "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
        candidate.display()
    );
}

pub fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(ralph_bin())
        .current_dir(dir)
        .env_remove("RUST_LOG")
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Create a ralph Command with proper environment isolation.
pub fn ralph_command(dir: &Path) -> Command {
    let mut cmd = Command::new(ralph_bin());
    cmd.current_dir(dir).env_remove("RUST_LOG");
    cmd
}

pub fn git_init(dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["init", "--quiet"])
        .status()
        .context("run git init")?;
    anyhow::ensure!(status.success(), "git init failed");

    let status = Command::new("git")
        .current_dir(dir)
        .args(["config", "user.name", "Ralph Test"])
        .status()
        .context("set local git user.name")?;
    anyhow::ensure!(status.success(), "git config user.name failed");

    let status = Command::new("git")
        .current_dir(dir)
        .args(["config", "user.email", "ralph-tests@example.invalid"])
        .status()
        .context("set local git user.email")?;
    anyhow::ensure!(status.success(), "git config user.email failed");

    let test_excludes_path = dir.join(".git").join("test-excludes");
    std::fs::write(&test_excludes_path, "").context("write test excludes file")?;
    let status = Command::new("git")
        .current_dir(dir)
        .args([
            "config",
            "core.excludesFile",
            test_excludes_path
                .to_str()
                .expect("utf-8 test excludes path"),
        ])
        .status()
        .context("override local core.excludesFile for test repo")?;
    anyhow::ensure!(status.success(), "git config core.excludesFile failed");

    let gitignore_path = dir.join(".gitignore");
    std::fs::write(
        &gitignore_path,
        ".ralph/lock\n.ralph/cache/\n.ralph/logs/\n",
    )?;

    let status = Command::new("git")
        .current_dir(dir)
        .args(["add", ".gitignore"])
        .status()
        .context("git add .gitignore")?;
    anyhow::ensure!(status.success(), "git add .gitignore failed");

    let status = Command::new("git")
        .current_dir(dir)
        .args(["commit", "--quiet", "-m", "add gitignore"])
        .status()
        .context("git commit .gitignore")?;
    anyhow::ensure!(status.success(), "git commit .gitignore failed");

    Ok(())
}

pub fn trust_project_commands(dir: &Path) -> Result<()> {
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;
    std::fs::write(
        ralph_dir.join("trust.jsonc"),
        r#"{
  "allow_project_commands": true,
  "trusted_at": "2026-03-07T00:00:00Z"
}
"#,
    )
    .context("write trust config")?;
    Ok(())
}

pub fn create_fake_runner(dir: &Path, runner: &str, script: &str) -> Result<PathBuf> {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir)?;
    let runner_path = bin_dir.join(runner);
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

pub fn create_executable_script(dir: &Path, name: &str, script: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    std::fs::write(&path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(path)
}

pub fn run_in_dir_raw(dir: &Path, bin: &str, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(bin)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|_| panic!("failed to execute binary: {}", bin));
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

pub fn git_add_all_commit(dir: &Path, message: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["add", "."])
        .status()
        .context("git add all")?;
    anyhow::ensure!(status.success(), "git add all failed");

    let status = Command::new("git")
        .current_dir(dir)
        .args(["commit", "--quiet", "-m", message])
        .status()
        .context("git commit")?;
    anyhow::ensure!(status.success(), "git commit failed");

    Ok(())
}

pub fn git_status_porcelain(dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["status", "--porcelain"])
        .output()
        .context("git status --porcelain")?;
    anyhow::ensure!(output.status.success(), "git status --porcelain failed");
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Initialize a ralph project in the given directory.
pub fn ralph_init(dir: &Path) -> Result<()> {
    let (status, stdout, stderr) = run_in_dir(dir, &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}

/// Run a closure with a prepended path segment.
///
/// The PATH is restored after the closure completes, even if it panics.
/// This is safe because we use env_lock to prevent concurrent access.
///
/// # Safety
/// This function uses unsafe to call `std::env::set_var`. The caller must ensure
/// that `env_lock()` is held to prevent concurrent modifications.
pub fn with_prepend_path<F, T>(prepend: &Path, f: F) -> T
where
    F: FnOnce() -> T,
{
    let original = std::env::var("PATH").unwrap_or_default();
    let new_path = if cfg!(windows) {
        format!("{};{}", prepend.display(), original)
    } else {
        format!("{}:{}", prepend.display(), original)
    };

    struct PathGuard(String);
    impl Drop for PathGuard {
        fn drop(&mut self) {
            #[allow(unused_unsafe)]
            unsafe {
                std::env::set_var("PATH", &self.0);
            }
        }
    }
    let _guard = PathGuard(original.clone());

    #[allow(unused_unsafe)]
    unsafe {
        std::env::set_var("PATH", &new_path);
    }
    f()
}
