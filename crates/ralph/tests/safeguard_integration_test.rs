//! Integration tests for LLM output safeguarding.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

mod test_support;

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    test_support::run_in_dir(dir, args)
}
fn git_init(dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["init", "--quiet"])
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
    let queue_path = ralph_dir.join("queue.jsonc");
    let done_path = ralph_dir.join("done.jsonc");

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

    std::fs::write(&queue_path, queue).context("write queue.jsonc")?;
    std::fs::write(&done_path, done).context("write done.jsonc")?;
    Ok(())
}

fn configure_runner(dir: &Path, runner: &str, model: &str, bin_path: Option<&Path>) -> Result<()> {
    let config_path = dir.join(".ralph/config.jsonc");
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
    if bin_path.is_some() {
        test_support::trust_project_commands(dir)?;
    }
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
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    // 1. Setup Ralph
    let (status, _stdout, _stderr) =
        run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
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
        stderr.contains("redacted stdout saved to"),
        "expected safeguard message in stderr, got:\n{}",
        stderr
    );

    // 7. Extract path and verify content
    // Find "redacted stdout saved to /path/to/output.txt"
    let pattern = "redacted stdout saved to ";
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
    let dir = test_support::temp_dir_outside_repo();
    git_init(dir.path())?;

    // 1. Setup Ralph
    let (status, _stdout, _stderr) =
        run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(status.success(), "ralph init failed");

    // 2. Create a runner that produces INVALID queue.json (corrupts it)
    // It should print valid LLM output but also mess up the file system.
    // The 'cat > /dev/null' drains stdin to prevent broken pipe errors.
    let script = "#!/bin/sh\ncat > /dev/null\necho 'VALUABLE_SCAN_OUTPUT'\necho 'corrupt' > .ralph/queue.jsonc\nexit 0\n";
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
        stderr.contains("redacted stdout saved to"),
        "expected safeguard message in stderr for scan, got:\n{}",
        stderr
    );

    // 6. Extract path and verify content
    let pattern = "redacted stdout saved to ";
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
