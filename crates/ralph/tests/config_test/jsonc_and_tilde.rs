//! JSONC runtime resolution and tilde-expansion tests.
//!
//! Purpose:
//! - JSONC runtime resolution and tilde-expansion tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::*;

#[test]
#[serial]
fn test_resolve_queue_path_expands_tilde_to_home() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~/myqueue.json"));

    let queue_path = config::resolve_queue_path(&repo_root, &cfg).unwrap();
    assert_eq!(queue_path, PathBuf::from("/custom/home/myqueue.json"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

// Tests for .jsonc file format support (RQ-0807)

#[test]
fn test_find_repo_root_via_ralph_queue_jsonc() {
    let dir = TempDir::new().expect("create temp dir");
    create_queue_jsonc(&dir, r#"{"version":1,"tasks":[]}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_via_ralph_config_jsonc() {
    let dir = TempDir::new().expect("create temp dir");
    create_config_jsonc(&dir, r#"{"version":2}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_resolve_queue_path_uses_jsonc_default_path() {
    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);
    fs::write(ralph_dir.join("queue.json"), r#"{"version":1,"tasks":[]}"#).unwrap();

    let queue_path = config::resolve_queue_path(dir.path(), &Config::default()).unwrap();
    assert_eq!(queue_path, ralph_dir.join("queue.jsonc"));
}

#[test]
fn test_resolve_done_path_uses_jsonc_default_path() {
    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);
    fs::write(ralph_dir.join("done.json"), r#"{"version":1,"tasks":[]}"#).unwrap();

    let done_path = config::resolve_done_path(dir.path(), &Config::default()).unwrap();
    assert_eq!(done_path, ralph_dir.join("done.jsonc"));
}

#[test]
fn test_project_config_path_uses_jsonc_path() {
    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);
    fs::write(ralph_dir.join("config.json"), r#"{"version":2}"#).unwrap();

    let config_path = config::project_config_path(dir.path());
    assert_eq!(config_path, ralph_dir.join("config.jsonc"));
}

#[test]
#[serial]
fn test_global_config_path_uses_jsonc_path() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = TempDir::new().expect("create temp dir");
    let xdg_config = dir.path().join(".config");
    let ralph_dir = xdg_config.join("ralph");
    fs::create_dir_all(&ralph_dir).expect("create xdg config dir");
    fs::write(ralph_dir.join("config.json"), r#"{"version":2}"#).unwrap();

    unsafe { env::set_var("XDG_CONFIG_HOME", &xdg_config) };
    let config_path = config::global_config_path();
    unsafe { env::remove_var("XDG_CONFIG_HOME") };

    assert_eq!(config_path, Some(ralph_dir.join("config.jsonc")));
}

#[test]
fn test_load_layer_accepts_jsonc_with_comments() {
    let dir = TempDir::new().expect("create temp dir");
    let config_path = dir.path().join("config.jsonc");

    // Write JSONC with comments
    let jsonc_content = r#"{
        // This is a single-line comment
        "version": 2,
        "agent": {
            /* Multi-line
               comment */
            "runner": "claude"
        }
    }"#;
    fs::write(&config_path, jsonc_content).expect("write config.jsonc");

    let layer = config::load_layer(&config_path).unwrap();
    assert_eq!(layer.version, Some(2));
    assert_eq!(layer.agent.runner, Some(Runner::Claude));
}

#[test]
fn test_load_queue_accepts_jsonc_with_comments() {
    let dir = TempDir::new().expect("create temp dir");
    let ralph_dir = setup_ralph_dir(&dir);
    let queue_path = ralph_dir.join("queue.jsonc");

    // Write JSONC with comments
    let jsonc_content = r#"{
        // Queue file with comments
        "version": 1,
        "tasks": [
            /* Task entry */
            {
                "id": "RQ-0001",
                "title": "Test task",
                "status": "todo",
                "tags": [],
                "scope": [],
                "evidence": [],
                "plan": [],
                "created_at": "2026-01-18T00:00:00Z",
                "updated_at": "2026-01-18T00:00:00Z"
            }
        ]
    }"#;
    fs::write(&queue_path, jsonc_content).expect("write queue.jsonc");

    let queue = ralph::queue::load_queue(&queue_path).unwrap();
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0001");
}

#[test]
#[serial]
fn test_resolve_done_path_expands_tilde_to_home() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from("~/mydone.json"));

    let done_path = config::resolve_done_path(&repo_root, &cfg).unwrap();
    assert_eq!(done_path, PathBuf::from("/custom/home/mydone.json"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn test_resolve_queue_path_expands_tilde_alone_to_home() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~"));

    let queue_path = config::resolve_queue_path(&repo_root, &cfg).unwrap();
    assert_eq!(queue_path, PathBuf::from("/custom/home"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn test_resolve_queue_path_does_not_join_when_tilde_expands() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    // When ~ expands to an absolute path, it should NOT be joined to repo_root
    let repo_root = PathBuf::from("/repo/root");
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~/queue.json"));

    let queue_path = config::resolve_queue_path(&repo_root, &cfg).unwrap();
    // Should be /custom/home/queue.json, NOT /repo/root/custom/home/queue.json
    assert_eq!(queue_path, PathBuf::from("/custom/home/queue.json"));
    assert!(!queue_path.to_string_lossy().contains("/repo/root"));

    // Restore HOME
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn test_resolve_queue_path_relative_when_home_unset() {
    let _guard = env_lock().lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    // Remove HOME - tilde should not expand, path treated as relative
    unsafe { env::remove_var("HOME") };

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("~/queue.json"));

    // When HOME is unset, ~/queue.json is treated as a relative path
    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, repo_root.join("~/queue.json"));

    // Restore HOME
    if let Some(v) = original_home {
        unsafe { env::set_var("HOME", v) }
    }
}
