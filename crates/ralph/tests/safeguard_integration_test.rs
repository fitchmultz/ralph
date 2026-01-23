// Integration tests for LLM output safeguarding.

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
    if !config_path.exists() {
        // Create basic config if missing
        let initial_config = r#"{ 
  "agent": {
    "runner": "codex",
    "model": "gpt-5.2-codex"
  },
  "queue": {
    "id_prefix": "RQ",
    "id_width": 4
  }
}"#;
        std::fs::write(&config_path, initial_config)?;
    }

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

fn create_fake_runner(dir: &Path, runner: &str, script: &str) -> Result<PathBuf> {
    let bin_dir = dir.join("bin");
    if !bin_dir.exists() {
        std::fs::create_dir(&bin_dir)?;
    }
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

#[test]
fn runner_fails_and_safeguards_stdout() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // 1. Setup Ralph
    let (status, _stdout, _stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(status.success(), "ralph init failed");

    // 2. Add a task
    write_valid_single_todo_queue(dir.path())?;

    // 3. Create a runner that prints and fails
    let script = "#!/bin/sh\necho 'VALUABLE_LLM_OUTPUT'\nexit 1\n";
    let runner_path = create_fake_runner(dir.path(), "codex", script)?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // 4. Commit setup
    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup"])
        .status()?;

    // 5. Run ralph
    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["run", "one"]);
    anyhow::ensure!(!status.success(), "expected run one to fail");

    // 6. Check for safeguard message in stderr
    anyhow::ensure!(
        stderr.contains("raw stdout saved to"),
        "expected safeguard message in stderr, got:\n{}",
        stderr
    );

    // 7. Extract path and verify content
    // Find "raw stdout saved to /path/to/output.txt"
    let pattern = "raw stdout saved to ";
    let start_idx = stderr.find(pattern).context("find safeguard pattern")? + pattern.len();
    let end_idx = stderr[start_idx..]
        .find(')')
        .context("find closing parenthesis")?
        + start_idx;
    let saved_path_str = &stderr[start_idx..end_idx];
    let saved_path = PathBuf::from(saved_path_str);

    anyhow::ensure!(
        saved_path.exists(),
        "safeguard file does not exist: {}",
        saved_path.display()
    );
    let content = std::fs::read_to_string(&saved_path)?;
    anyhow::ensure!(
        content.contains("VALUABLE_LLM_OUTPUT"),
        "safeguard file content mismatch"
    );

    Ok(())
}

#[test]
fn scan_fails_validation_and_safeguards_stdout() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // 1. Setup Ralph
    let (status, _stdout, _stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(status.success(), "ralph init failed");

    // 2. Create a runner that produces INVALID queue.json (corrupts it)
    // It should print valid LLM output but also mess up the file system.
    let script =
        "#!/bin/sh\necho 'VALUABLE_SCAN_OUTPUT'\necho 'corrupt' > .ralph/queue.json\nexit 0\n";
    let runner_path = create_fake_runner(dir.path(), "codex", script)?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // 3. Commit setup
    Command::new("git")
        .current_dir(dir.path())
        .args(["add", "."])
        .status()?;
    Command::new("git")
        .current_dir(dir.path())
        .args(["commit", "-m", "setup"])
        .status()?;

    // 4. Run ralph scan
    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["scan", "--focus", "test"]);
    anyhow::ensure!(!status.success(), "expected scan to fail due to validation");

    // 5. Check for safeguard message in stderr
    anyhow::ensure!(
        stderr.contains("raw stdout saved to"),
        "expected safeguard message in stderr for scan, got:\n{}",
        stderr
    );

    // 6. Extract path and verify content
    let pattern = "raw stdout saved to ";
    let start_idx = stderr.find(pattern).context("find safeguard pattern")? + pattern.len();
    let end_idx = stderr[start_idx..]
        .find(')')
        .context("find closing parenthesis")?
        + start_idx;
    let saved_path_str = &stderr[start_idx..end_idx];
    let saved_path = PathBuf::from(saved_path_str);

    anyhow::ensure!(
        saved_path.exists(),
        "safeguard file does not exist: {}",
        saved_path.display()
    );
    let content = std::fs::read_to_string(&saved_path)?;
    anyhow::ensure!(
        content.contains("VALUABLE_SCAN_OUTPUT"),
        "safeguard file content mismatch"
    );

    Ok(())
}
