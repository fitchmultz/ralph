//! Repository-root and queue/done path resolution tests.
//!
//! Purpose:
//! - Repository-root and queue/done path resolution tests.
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
fn test_find_repo_root_via_ralph_queue() {
    let dir = TempDir::new().expect("create temp dir");
    create_queue_jsonc(&dir, r#"{"version":1,"tasks":[]}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_via_ralph_config() {
    let dir = TempDir::new().expect("create temp dir");
    create_config_jsonc(&dir, r#"{"version":2}"#);

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_via_git() {
    let dir = TempDir::new().expect("create temp dir");
    let git_dir = dir.path().join(".git");
    fs::create_dir_all(&git_dir).expect("create .git dir");

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_nested() {
    let dir = TempDir::new().expect("create temp dir");
    create_queue_jsonc(&dir, r#"{"version":1,"tasks":[]}"#);

    let nested = dir.path().join("nested").join("deep");
    fs::create_dir_all(&nested).expect("create nested dirs");

    let repo_root = config::find_repo_root(&nested);
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_find_repo_root_fallback_to_start() {
    let dir = test_support::temp_dir_outside_repo();
    // No .ralph or .git directory

    let repo_root = config::find_repo_root(dir.path());
    assert_eq!(repo_root, dir.path());
}

#[test]
fn test_project_config_path() {
    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();

    let config_path = config::project_config_path(repo_root);
    assert_eq!(config_path, repo_root.join(".ralph").join("config.jsonc"));
}

#[test]
fn test_global_config_path_xdg() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = TempDir::new().expect("create temp dir");
    let xdg_config = dir.path().join(".config");
    fs::create_dir_all(xdg_config.join("ralph")).expect("create xdg config dir");

    unsafe { env::set_var("XDG_CONFIG_HOME", &xdg_config) };
    let config_path = config::global_config_path();
    unsafe { env::remove_var("XDG_CONFIG_HOME") };

    assert!(config_path.is_some());
    assert_eq!(
        config_path.unwrap(),
        xdg_config.join("ralph").join("config.jsonc")
    );
}

#[test]
fn test_global_config_path_home() {
    let _guard = env_lock().lock().expect("env lock");
    let dir = TempDir::new().expect("create temp dir");
    let home_config = dir.path().join(".config").join("ralph");
    fs::create_dir_all(&home_config).expect("create home config dir");

    unsafe { env::set_var("HOME", dir.path()) };
    unsafe { env::remove_var("XDG_CONFIG_HOME") };
    let config_path = config::global_config_path();
    unsafe { env::remove_var("HOME") };

    assert!(config_path.is_some());
    assert_eq!(
        config_path.unwrap(),
        dir.path()
            .join(".config")
            .join("ralph")
            .join("config.jsonc")
    );
}

#[test]
fn test_global_config_path_none_if_no_home() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { env::remove_var("XDG_CONFIG_HOME") };
    unsafe { env::remove_var("HOME") };
    let config_path = config::global_config_path();
    assert!(config_path.is_none());
}

#[test]
#[serial]
fn test_resolve_queue_path_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let cfg = Config::default();

    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, repo_root.join(".ralph/queue.jsonc"));
}

#[test]
#[serial]
fn test_resolve_queue_path_custom_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from("custom/queue.json"));

    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, repo_root.join("custom/queue.json"));
}

#[test]
#[serial]
fn test_resolve_queue_path_absolute() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let absolute = test_support::portable_abs_path("absolute/queue.json");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(absolute.clone());

    let queue_path = config::resolve_queue_path(repo_root, &cfg).unwrap();
    assert_eq!(queue_path, absolute);
}

#[test]
#[serial]
fn test_resolve_queue_path_empty_fails() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.file = Some(PathBuf::from(""));

    let result = config::resolve_queue_path(repo_root, &cfg);
    assert!(result.is_err());
}

#[test]
#[serial]
fn test_resolve_done_path_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let cfg = Config::default();

    let done_path = config::resolve_done_path(repo_root, &cfg).unwrap();
    assert_eq!(done_path, repo_root.join(".ralph/done.jsonc"));
}

#[test]
#[serial]
fn test_resolve_done_path_custom_relative() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from("custom/done.json"));

    let done_path = config::resolve_done_path(repo_root, &cfg).unwrap();
    assert_eq!(done_path, repo_root.join("custom/done.json"));
}

#[test]
#[serial]
fn test_resolve_done_path_absolute() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let absolute = test_support::portable_abs_path("absolute/done.json");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(absolute.clone());

    let done_path = config::resolve_done_path(repo_root, &cfg).unwrap();
    assert_eq!(done_path, absolute);
}

#[test]
#[serial]
fn test_resolve_done_path_empty_fails() {
    let _guard = env_lock().lock().expect("env lock");

    let dir = TempDir::new().expect("create temp dir");
    let repo_root = dir.path();
    let mut cfg = Config::default();
    cfg.queue.done_file = Some(PathBuf::from(""));

    let result = config::resolve_done_path(repo_root, &cfg);
    assert!(result.is_err());
}
