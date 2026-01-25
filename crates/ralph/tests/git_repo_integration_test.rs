//! Integration tests for ralph CLI behavior against real git repositories.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

mod test_support;

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
    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let queue = r#"{
  "version": 1,
  "tasks": [
    {
      "id": "RQ-0001",
      "status": "todo",
      "title": "Test task",
      "tags": ["rust"],
      "scope": ["crates/ralph"],
      "evidence": ["integration test fixture"],
      "plan": ["run preflight"],
      "request": "integration test",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    }
  ]
}"#;

    let done = r#"{
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(&queue_path, queue).context("write queue.json")?;
    std::fs::write(&done_path, done).context("write done.json")?;
    Ok(())
}

fn configure_runner(dir: &Path, runner: &str, model: &str, bin_path: Option<&Path>) -> Result<()> {
    let config_path = dir.join(".ralph/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).context("read config")?)
            .context("parse config")?;
    if config.get("agent").is_none() {
        config["agent"] = serde_json::json!({});
    }
    let agent = config
        .get_mut("agent")
        .and_then(|value| value.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("config missing agent section"))?;
    agent.insert("runner".to_string(), serde_json::json!(runner));
    agent.insert("model".to_string(), serde_json::json!(model));
    agent.insert("phases".to_string(), serde_json::json!(1));
    if let Some(path) = bin_path {
        let key = match runner {
            "codex" => "codex_bin",
            "opencode" => "opencode_bin",
            "gemini" => "gemini_bin",
            "claude" => "claude_bin",
            _ => return Err(anyhow::anyhow!("unsupported runner: {}", runner)),
        };
        agent.insert(
            key.to_string(),
            serde_json::json!(path.to_string_lossy().to_string()),
        );
    }
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize config")?,
    )
    .context("write config")?;
    Ok(())
}

fn configure_ci_gate(dir: &Path, command: Option<&str>, enabled: Option<bool>) -> Result<()> {
    let config_path = dir.join(".ralph/config.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).context("read config")?)
            .context("parse config")?;
    if config.get("agent").is_none() {
        config["agent"] = serde_json::json!({});
    }
    let agent = config
        .get_mut("agent")
        .and_then(|value| value.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("config missing agent section"))?;
    if let Some(command) = command {
        agent.insert("ci_gate_command".to_string(), serde_json::json!(command));
    }
    if let Some(enabled) = enabled {
        agent.insert("ci_gate_enabled".to_string(), serde_json::json!(enabled));
    }
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize config")?,
    )
    .context("write config")?;
    Ok(())
}

fn create_fake_runner(dir: &Path, runner: &str, script: &str) -> Result<PathBuf> {
    let bin_dir = dir.join("bin");
    std::fs::create_dir(&bin_dir)?;
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

fn create_executable_script(dir: &Path, name: &str, script: &str) -> Result<PathBuf> {
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

#[test]
fn init_and_validate_work_in_fresh_git_repo() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", None)?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "validate"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue validate failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_refuses_to_run_when_repo_is_dirty_and_a_todo_exists() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", None)?;

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
    let dir = test_support::temp_dir_outside_repo();
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

    let runner_path = create_fake_runner(dir.path(), "codex", "#!/bin/sh\nexit 0\n")?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // Use --force to bypass the dirty repo check.
    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
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
    let dir = test_support::temp_dir_outside_repo();
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

    // Create a dummy Makefile for post_run_supervise.
    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo 'CI passed'\n")
        .context("write Makefile")?;

    let runner_path = create_fake_runner(dir.path(), "codex", "#!/bin/sh\nexit 0\n")?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // 4. Run `ralph run one` with the fake runner
    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup test env"])
        .status()?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
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
    let done_content = std::fs::read_to_string(dir.path().join(".ralph/done.json"))?;
    anyhow::ensure!(
        done_content.contains("RQ-0001"),
        "task should be moved to done"
    );

    Ok(())
}

#[test]
fn scan_refuses_to_run_when_repo_is_dirty() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
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

#[test]
fn run_one_reverts_changes_when_ci_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Add a task to the queue.
    write_valid_single_todo_queue(dir.path())?;

    // Create a Makefile with a failing `ci` target.
    let makefile_content = r#"ci:
	@echo 'CI failing'
	exit 1
"#;
    std::fs::write(dir.path().join("Makefile"), makefile_content).context("write Makefile")?;

    // Create a "dirty runner" that creates a file and exits 0.
    let dirty_file = dir.path().join("dirty-file.txt");
    let script = format!(
        "#!/bin/sh\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path =
        create_fake_runner(dir.path(), "codex", &script).context("write runner script")?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // Commit the setup so the repo starts clean.
    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()
        .context("git add")?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup test env"])
        .status()
        .context("git commit")?;

    // Run `ralph run one`.
    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("run")
        .arg("one")
        .arg("--git-revert-mode")
        .arg("enabled")
        .output()
        .context("run ralph run one")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Assert: execution fails.
    anyhow::ensure!(
        !output.status.success(),
        "expected run one to fail due to CI\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Assert: stderr mentions CI failure.
    anyhow::ensure!(
        stderr.contains("CI gate failed") || stderr.contains("CI failed"),
        "expected CI failure message in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Assert: dirty file does NOT exist (changes were reverted).
    anyhow::ensure!(
        !dirty_file.exists(),
        "dirty file should not exist after CI failure and rollback"
    );

    // Assert: repo is clean (no uncommitted changes).
    let git_status = Command::new("git")
        .current_dir(dir.path())
        .args(["status", "--porcelain"])
        .output()
        .context("git status")?;
    let status_output = String::from_utf8_lossy(&git_status.stdout);
    anyhow::ensure!(
        status_output.trim().is_empty(),
        "repo should be clean after rollback, but git status showed:\n{status_output}"
    );

    Ok(())
}

#[test]
fn run_one_keeps_changes_when_ci_fails_and_git_revert_mode_disabled() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    write_valid_single_todo_queue(dir.path())?;

    let makefile_content = r#"ci:
\t@echo 'CI failing'
\texit 1
"#;
    std::fs::write(dir.path().join("Makefile"), makefile_content).context("write Makefile")?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let script = format!(
        "#!/bin/sh\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path =
        create_fake_runner(dir.path(), "codex", &script).context("write runner script")?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()
        .context("git add")?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup test env"])
        .status()
        .context("git commit")?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("run")
        .arg("one")
        .arg("--git-revert-mode")
        .arg("disabled")
        .output()
        .context("run ralph run one")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    anyhow::ensure!(
        !output.status.success(),
        "expected run one to fail due to CI\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("CI gate failed") || stderr.contains("CI failed"),
        "expected CI failure message in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        dirty_file.exists(),
        "dirty file should remain when git revert is disabled"
    );

    Ok(())
}

#[test]
fn run_one_keeps_changes_when_ci_fails_and_git_revert_mode_ask_non_tty() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    write_valid_single_todo_queue(dir.path())?;

    let makefile_content = r#"ci:
\t@echo 'CI failing'
\texit 1
"#;
    std::fs::write(dir.path().join("Makefile"), makefile_content).context("write Makefile")?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let script = format!(
        "#!/bin/sh\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path =
        create_fake_runner(dir.path(), "codex", &script).context("write runner script")?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()
        .context("git add")?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup test env"])
        .status()
        .context("git commit")?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("run")
        .arg("one")
        .arg("--git-revert-mode")
        .arg("ask")
        .output()
        .context("run ralph run one")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    anyhow::ensure!(
        !output.status.success(),
        "expected run one to fail due to CI\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("CI gate failed") || stderr.contains("CI failed"),
        "expected CI failure message in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        dirty_file.exists(),
        "dirty file should remain when ask mode runs non-interactively"
    );

    Ok(())
}

#[test]
fn run_one_fails_when_custom_ci_gate_command_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    write_valid_single_todo_queue(dir.path())?;

    let script = "#!/bin/sh\necho 'CI failing'\nexit 2\n";
    create_executable_script(dir.path(), "ci-gate.sh", script)?;
    configure_ci_gate(dir.path(), Some("./ci-gate.sh"), Some(true))?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let runner_script = format!(
        "#!/bin/sh\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path =
        create_fake_runner(dir.path(), "codex", &runner_script).context("write runner script")?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()
        .context("git add")?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup test env"])
        .status()
        .context("git commit")?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("run")
        .arg("one")
        .arg("--git-revert-mode")
        .arg("disabled")
        .output()
        .context("run ralph run one")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    anyhow::ensure!(
        !output.status.success(),
        "expected run one to fail due to CI\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("CI gate failed") || stderr.contains("CI failed"),
        "expected CI failure message in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("./ci-gate.sh"),
        "expected CI gate command in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_succeeds_when_ci_gate_disabled() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    write_valid_single_todo_queue(dir.path())?;

    let script = "#!/bin/sh\necho 'CI failing'\nexit 2\n";
    create_executable_script(dir.path(), "ci-gate.sh", script)?;
    configure_ci_gate(dir.path(), Some("./ci-gate.sh"), Some(false))?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let runner_script = format!(
        "#!/bin/sh\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path =
        create_fake_runner(dir.path(), "codex", &runner_script).context("write runner script")?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()
        .context("git add")?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup test env"])
        .status()
        .context("git commit")?;

    let output = Command::new(ralph_bin())
        .current_dir(dir.path())
        .arg("run")
        .arg("one")
        .arg("--git-revert-mode")
        .arg("disabled")
        .output()
        .context("run ralph run one")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    anyhow::ensure!(
        output.status.success(),
        "expected run one to succeed with CI gate disabled\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let done_content = std::fs::read_to_string(dir.path().join(".ralph/done.json"))?;
    anyhow::ensure!(
        done_content.contains("RQ-0001"),
        "task should be moved to done when CI gate is disabled"
    );

    let git_status = Command::new("git")
        .current_dir(dir.path())
        .args(["status", "--porcelain"])
        .output()
        .context("git status")?;
    let status_output = String::from_utf8_lossy(&git_status.stdout);
    anyhow::ensure!(
        status_output.trim().is_empty(),
        "repo should be clean after successful run, but git status showed:\n{status_output}"
    );

    Ok(())
}
