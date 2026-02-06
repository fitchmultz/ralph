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
use serde_json::Value;
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
        description: None,
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
        started_at: None,
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
        .env("RALPH_REPO_ROOT_OVERRIDE", dir)
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
    std::fs::write(
        &gitignore_path,
        ".ralph/lock\n.ralph/cache/\n.ralph/logs/\n",
    )?;
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

/// Update `.ralph/config.json` to set `agent.runner`, `agent.model`, and `agent.phases`.
///
/// Assumptions:
/// - `ralph init` has already been run (so `.ralph/config.json` exists).
pub fn configure_agent_runner_model_phases(
    dir: &Path,
    runner: &str,
    model: &str,
    phases: u8,
) -> Result<()> {
    let config_path = dir.join(".ralph/config.json");
    let config_str = std::fs::read_to_string(&config_path).context("read .ralph/config.json")?;
    let mut config: Value =
        serde_json::from_str(&config_str).context("parse .ralph/config.json")?;

    if config.get("agent").is_none() {
        config["agent"] = serde_json::json!({});
    }

    let agent = config["agent"]
        .as_object_mut()
        .context("config.agent is not an object")?;
    agent.insert("runner".to_string(), serde_json::json!(runner));
    agent.insert("model".to_string(), serde_json::json!(model));
    agent.insert("phases".to_string(), serde_json::json!(phases));

    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize .ralph/config.json")?,
    )
    .context("write .ralph/config.json")?;
    Ok(())
}

/// Write `.ralph/cache/execution_history.json` with a single v1 entry.
///
/// Assumptions:
/// - The history schema uses `secs`/`nanos` objects for durations.
/// - Callers provide consistent totals (this helper does not cross-check).
/// - The entry is always written with `phase_count = 3`.
pub fn write_execution_history_v1_single_sample(
    dir: &Path,
    runner: &str,
    model: &str,
    total_secs: u64,
    planning_secs: u64,
    implementation_secs: u64,
    review_secs: u64,
) -> Result<()> {
    let history = serde_json::json!({
      "version": 1,
      "entries": [
        {
          "timestamp": "2026-02-01T00:00:00Z",
          "task_id": "RQ-9999",
          "runner": runner,
          "model": model,
          "phase_count": 3,
          "phase_durations": {
            "planning": { "secs": planning_secs, "nanos": 0 },
            "implementation": { "secs": implementation_secs, "nanos": 0 },
            "review": { "secs": review_secs, "nanos": 0 }
          },
          "total_duration": { "secs": total_secs, "nanos": 0 }
        }
      ]
    });

    let cache_dir = dir.join(".ralph/cache");
    std::fs::create_dir_all(&cache_dir).context("create .ralph/cache")?;
    std::fs::write(
        cache_dir.join("execution_history.json"),
        serde_json::to_string_pretty(&history).context("serialize execution_history.json")?,
    )
    .context("write execution_history.json")?;
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

/// Initialize a ralph project in the given directory.
pub fn ralph_init(dir: &Path) -> Result<()> {
    let (status, stdout, stderr) = run_in_dir(dir, &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    Ok(())
}

/// Write a queue file with the given tasks.
pub fn write_queue(dir: &Path, tasks: &[Task]) -> Result<()> {
    let queue = QueueFile {
        version: 1,
        tasks: tasks.to_vec(),
    };
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let queue_path = ralph_dir.join("queue.json");
    let json = serde_json::to_string_pretty(&queue)?;
    std::fs::write(&queue_path, json).with_context(|| "write queue.json".to_string())?;
    Ok(())
}

/// Write a done file with the given tasks.
pub fn write_done(dir: &Path, tasks: &[Task]) -> Result<()> {
    let done = QueueFile {
        version: 1,
        tasks: tasks.to_vec(),
    };
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;
    let done_path = ralph_dir.join("done.json");
    let json = serde_json::to_string_pretty(&done)?;
    std::fs::write(&done_path, json).with_context(|| "write done.json".to_string())?;
    Ok(())
}

/// Read the queue file from the given directory.
pub fn read_queue(dir: &Path) -> Result<QueueFile> {
    let queue_path = dir.join(".ralph/queue.json");
    let raw = std::fs::read_to_string(&queue_path).context("read queue.json")?;
    serde_json::from_str(&raw).context("parse queue.json")
}

/// Read the done file from the given directory.
pub fn read_done(dir: &Path) -> Result<QueueFile> {
    let done_path = dir.join(".ralph/done.json");
    let raw = std::fs::read_to_string(&done_path).context("read done.json")?;
    serde_json::from_str(&raw).context("parse done.json")
}

/// Normalize CLI output for stable snapshots.
///
/// Applies filters to make output deterministic across runs:
/// - Normalizes line endings (\r\n → \n)
/// - Strips ANSI escape codes
/// - Replaces dates with <DATE> placeholder
pub fn normalize_for_snapshot(output: &str) -> String {
    use regex::Regex;

    let mut result = output.to_string();

    // Normalize line endings
    result = result.replace("\r\n", "\n");

    // Strip ANSI escape codes
    let ansi_regex = Regex::new(r"\x1b\[[0-9;]*m").expect("valid regex");
    result = ansi_regex.replace_all(&result, "").to_string();

    // Replace dates with placeholder
    let date_regex = Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").expect("valid regex");
    result = date_regex.replace_all(&result, "<DATE>").to_string();

    result
}

/// Bind `insta` settings suitable for CLI snapshots.
pub fn with_insta_settings<T>(f: impl FnOnce() -> T) -> T {
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(f)
}
