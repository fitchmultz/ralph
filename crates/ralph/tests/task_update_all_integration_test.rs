//! Integration tests for `ralph task update` without a task ID.

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

fn configure_runner(dir: &Path, runner: &str, model: &str, bin_path: Option<&Path>) -> Result<()> {
    let config_path = dir.join(".ralph/config.json");
    if !config_path.exists() {
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
        std::fs::create_dir_all(dir.join(".ralph")).context("create .ralph dir")?;
        std::fs::write(&config_path, initial_config).context("write initial config")?;
    }

    let raw = std::fs::read_to_string(&config_path).context("read config")?;
    let mut config: serde_json::Value = serde_json::from_str(&raw).context("parse config")?;

    if !config.get("agent").is_some_and(|value| value.is_object()) {
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

fn write_queue_with_two_tasks(dir: &Path) -> Result<()> {
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
      "title": "First task",
      "tags": ["test"],
      "scope": ["crates/ralph"],
      "evidence": ["integration test"],
      "plan": ["step one"],
      "notes": [],
      "request": "first request",
      "created_at": "2026-01-18T00:00:00Z",
      "updated_at": "2026-01-18T00:00:00Z"
    },
    {
      "id": "RQ-0002",
      "status": "todo",
      "title": "Second task",
      "tags": ["test"],
      "scope": ["crates/ralph"],
      "evidence": ["integration test"],
      "plan": ["step one"],
      "notes": [],
      "request": "second request",
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

fn write_empty_queue(dir: &Path) -> Result<()> {
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;
    let queue_path = ralph_dir.join("queue.json");
    let done_path = ralph_dir.join("done.json");

    let queue = r#"{ 
  "version": 1,
  "tasks": []
}"#;

    let done = r#"{ 
  "version": 1,
  "tasks": []
}"#;

    std::fs::write(&queue_path, queue).context("write queue.json")?;
    std::fs::write(&done_path, done).context("write done.json")?;
    Ok(())
}

#[test]
fn task_update_without_id_updates_all_tasks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    write_queue_with_two_tasks(dir.path())?;

    let script = "#!/bin/sh\ncat >/dev/null\necho run >> .ralph/runner_calls.txt\nexit 0\n";
    let runner_path = create_fake_runner(dir.path(), "codex", script)?;
    configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["task", "update"]);
    anyhow::ensure!(
        status.success(),
        "expected task update to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let calls_path = dir.path().join(".ralph/runner_calls.txt");
    let calls = std::fs::read_to_string(&calls_path).context("read runner calls")?;
    let call_count = calls.lines().count();
    anyhow::ensure!(
        call_count == 2,
        "expected runner to be invoked for each task, got {call_count}"
    );

    Ok(())
}

#[test]
fn task_update_without_id_fails_on_empty_queue() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    write_empty_queue(dir.path())?;

    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["task", "update"]);
    anyhow::ensure!(!status.success(), "expected task update to fail");
    anyhow::ensure!(
        stderr.contains("No tasks in queue to update"),
        "expected empty-queue error, got:\n{stderr}"
    );

    Ok(())
}
