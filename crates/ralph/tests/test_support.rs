//! Shared helpers for integration tests.
//!
//! This module centralizes test-only helpers that are reused across multiple integration-test
//! crates under `crates/ralph/tests/`.
//!
//! ## Why `dead_code` is allowed here
//!
//! Each file under `crates/ralph/tests/` is compiled as its own integration-test crate. This
//! module is `mod`-included by many different test crates, each using a different subset of
//! helpers below. Without a module-level `dead_code` allow, those crates would produce noisy
//! warnings for helpers they don't happen to call.
#![allow(dead_code)]

use anyhow::{Context, Result};
use ralph::config;
use ralph::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

pub fn path_has_repo_markers(path: &Path) -> bool {
    path.ancestors()
        .any(|dir| dir.join(".git").exists() || dir.join(".ralph").is_dir())
}

pub fn find_non_repo_temp_base() -> PathBuf {
    let cwd = env::current_dir().expect("resolve current dir");
    let repo_root = config::find_repo_root(&cwd);
    let mut candidates = Vec::new();
    if let Some(parent) = repo_root.parent() {
        candidates.push(parent.to_path_buf());
    }
    candidates.push(env::temp_dir());
    candidates.push(PathBuf::from("/tmp"));

    for candidate in candidates {
        if candidate.as_os_str().is_empty() {
            continue;
        }
        if !path_has_repo_markers(&candidate) {
            return candidate;
        }
    }

    repo_root
}

pub fn temp_dir_outside_repo() -> TempDir {
    let base = find_non_repo_temp_base();
    std::fs::create_dir_all(&base).expect("ensure temp base exists");
    TempDir::new_in(&base).expect("create temp dir outside repo")
}

pub fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Helper to create a test task.
///
/// The fields are intentionally fully-populated so contract/rendering tests can rely on realistic
/// data without repeating boilerplate.
pub fn make_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    let completed_at = match status {
        TaskStatus::Done | TaskStatus::Rejected => Some("2026-01-19T00:00:00Z".to_string()),
        _ => None,
    };
    Task {
        id: id.to_string(),
        title: title.to_string(),
        status,
        priority: TaskPriority::Medium,
        tags: vec!["test".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["test evidence".to_string()],
        plan: vec!["test plan".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-19T00:00:00Z".to_string()),
        updated_at: Some("2026-01-19T00:00:00Z".to_string()),
        completed_at,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
    }
}

/// Helper to create a test queue with multiple tasks.
pub fn make_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

/// Rendering-focused task fixture.
///
/// This matches historical fixtures embedded in `tui_rendering_test.rs`:
/// - `plan` has two steps (to exercise multi-step rendering)
/// - `completed_at` is intentionally `None` even for Done/Rejected tasks (rendering tests
///   explicitly control timestamp sections when needed)
pub fn make_render_test_task(id: &str, title: &str, status: TaskStatus) -> Task {
    let mut task = make_test_task(id, title, status);
    task.plan = vec![
        "test plan step 1".to_string(),
        "test plan step 2".to_string(),
    ];
    task.completed_at = None;
    task
}

/// Rendering-focused queue fixture (uses `make_render_test_task`).
pub fn make_render_test_queue() -> QueueFile {
    QueueFile {
        version: 1,
        tasks: vec![
            make_render_test_task("RQ-0001", "First Task", TaskStatus::Todo),
            make_render_test_task("RQ-0002", "Second Task", TaskStatus::Doing),
            make_render_test_task("RQ-0003", "Third Task", TaskStatus::Done),
        ],
    }
}

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
        // Remove repo root override so child processes don't inherit the parent's repo
        .env_remove("RALPH_REPO_ROOT_OVERRIDE")
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

pub fn git_init(dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["init", "--quiet"])
        .status()
        .context("run git init")?;
    anyhow::ensure!(status.success(), "git init failed");

    let gitignore_path = dir.join(".gitignore");
    std::fs::write(&gitignore_path, ".ralph/lock\n.ralph/cache/\n")?;
    Command::new("git")
        .current_dir(dir)
        .args(["add", ".gitignore"])
        .status()?;
    Command::new("git")
        .current_dir(dir)
        .args(["commit", "--quiet", "-m", "add gitignore"])
        .status()?;

    Ok(())
}

pub fn write_valid_single_todo_queue(dir: &Path) -> Result<()> {
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

pub fn configure_runner(
    dir: &Path,
    runner: &str,
    model: &str,
    bin_path: Option<&Path>,
) -> Result<()> {
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

pub fn configure_ci_gate(dir: &Path, command: Option<&str>, enabled: Option<bool>) -> Result<()> {
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
