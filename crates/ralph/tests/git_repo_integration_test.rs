use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use tempfile::TempDir;

fn ralph_bin() -> PathBuf {
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

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(ralph_bin())
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn git_init(dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["init"])
        .status()
        .context("run git init")?;
    anyhow::ensure!(status.success(), "git init failed");

    let gitignore_path = dir.join(".gitignore");
    std::fs::write(&gitignore_path, ".ralph/lock\n")?;
    Command::new("git")
        .current_dir(dir)
        .args(["add", ".gitignore"])
        .status()?;
    Command::new("git")
        .current_dir(dir)
        .args(["commit", "-m", "add gitignore"])
        .status()?;

    Ok(())
}

fn write_valid_single_todo_queue(dir: &Path) -> Result<()> {
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;
    let queue_path = ralph_dir.join("queue.yaml");
    let done_path = ralph_dir.join("done.yaml");

    let queue = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Test task
    tags:
      - rust
    scope:
      - crates/ralph
    evidence:
      - integration test fixture
    plan:
      - run preflight
    request: integration test
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;

    let done = r#"version: 1
tasks: []
"#;

    std::fs::write(&queue_path, queue).context("write queue.yaml")?;
    std::fs::write(&done_path, done).context("write done.yaml")?;
    Ok(())
}

fn write_done_with_mapping_notes(dir: &Path) -> Result<()> {
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;
    let done_path = ralph_dir.join("done.yaml");

    let done = r#"version: 1
tasks:
  - id: RQ-0099
    status: done
    title: Done task
    tags:
      - rust
    scope:
      - crates/ralph
    evidence:
      - done evidence
    plan:
      - done plan
    notes:
      - key: value
    request: test
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
    completed_at: 2026-01-18T00:00:00Z
"#;

    std::fs::write(&done_path, done).context("write done.yaml")?;
    Ok(())
}

#[test]
fn init_and_validate_work_in_fresh_git_repo() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "validate"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue validate failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_refuses_to_run_when_repo_is_dirty_and_a_todo_exists() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Ensure there is a todo item so run_cmd hits the clean-repo preflight.
    write_valid_single_todo_queue(dir.path())?;

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["run", "one"]);
    anyhow::ensure!(
        !status.success(),
        "expected run one to fail on dirty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("repo is dirty"),
        "expected dirty repo error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_succeeds_when_repo_is_dirty_and_force_is_used() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Ensure there is a todo item.
    write_valid_single_todo_queue(dir.path())?;

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    // Create a dummy Makefile for post_run_supervise
    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo 'CI passed'\n")
        .context("write Makefile")?;

    // Create a fake runner that succeeds (does nothing)
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let runner_path = bin_dir.join("codex");
    let script = "#!/bin/sh\nexit 0\n";
    std::fs::write(&runner_path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&runner_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&runner_path, perms)?;
    }

    let path_env = std::env::join_paths(std::iter::once(bin_dir).chain(std::env::split_paths(
        &std::env::var("PATH").unwrap_or_default(),
    )))?;

    // Use --force to bypass the dirty repo check.
    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .env("PATH", path_env)
        .arg("--force")
        .arg("run")
        .arg("one")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    anyhow::ensure!(
        output.status.success(),
        "run one failed with --force on dirty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_succeeds_without_upstream_and_warns() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // This mimics the production environment where .ralph/lock is ignored,
    // preventing the 'repo is dirty' check in run_one from failing due to the lock file.
    let gitignore_path = dir.path().join(".gitignore");
    std::fs::write(&gitignore_path, ".ralph/lock\n")?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["add", ".gitignore"])
        .status()?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "add gitignore"])
        .status()?;

    // 1. Setup Ralph
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // 2. Add a task
    write_valid_single_todo_queue(dir.path())?;

    // 3. Create a fake runner that succeeds (does nothing)
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir(&bin_dir)?;
    let runner_path = bin_dir.join("codex");

    let script = "#!/bin/sh\nexit 0\n";
    std::fs::write(&runner_path, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&runner_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&runner_path, perms)?;
    }

    // 4. Run `ralph run one` with the fake runner on PATH
    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup test env"])
        .status()?;

    let path_env = std::env::join_paths(std::iter::once(bin_dir).chain(std::env::split_paths(
        &std::env::var("PATH").unwrap_or_default(),
    )))?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .env("PATH", path_env)
        .arg("run")
        .arg("one")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    anyhow::ensure!(
        output.status.success(),
        "run one failed but should have succeeded (soft push failure)\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("skipping push (no upstream configured)"),
        "expected warning about skipping push\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify task was actually marked done and archived (supervisor logic)
    let done_content = std::fs::read_to_string(dir.path().join(".ralph/done.yaml"))?;
    anyhow::ensure!(
        done_content.contains("RQ-0001"),
        "task should be moved to done"
    );

    Ok(())
}

#[test]
fn queue_validate_repairs_done_yaml_mapping_notes() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    write_valid_single_todo_queue(dir.path())?;
    write_done_with_mapping_notes(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "validate"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue validate failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let done_content = std::fs::read_to_string(dir.path().join(".ralph/done.yaml"))?;
    anyhow::ensure!(
        done_content.contains("- 'key: value'") || done_content.contains("- \"key: value\""),
        "expected done.yaml to be repaired and quoted"
    );

    Ok(())
}

#[test]
fn scan_refuses_to_run_when_repo_is_dirty() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["scan", "--focus", "security"]);
    anyhow::ensure!(
        !status.success(),
        "expected scan to fail on dirty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("repo is dirty"),
        "expected dirty repo error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
