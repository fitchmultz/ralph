//! Gitignored allowlist sync tests for parallel workspace state synchronization.
//!
//! Purpose:
//! - Gitignored allowlist sync tests for parallel workspace state synchronization.
//!
//! Responsibilities:
//! - Verify `.env*` allowlist sync behavior for ignored repo files.
//! - Verify ignored directories and worker-parent paths are excluded.
//! - Verify unit-level filtering behavior for gitignored path normalization.
//!
//! Non-scope:
//! - `.ralph` runtime-tree recursion coverage.
//! - Custom bookkeeping path mapping scenarios.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Test names and assertions match the prior flat suite exactly.
//! - Filtering expectations remain narrow by design (`.env*` only).

use super::*;
use log::{LevelFilter, Log, Metadata, Record};
use serial_test::serial;
use std::sync::{Mutex, OnceLock};

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

struct GitignoredTestLogger;

static LOGGER: GitignoredTestLogger = GitignoredTestLogger;
static LOGGER_STATE: OnceLock<LoggerState> = OnceLock::new();
static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoggerState {
    TestLogger,
    OtherLogger,
}

impl Log for GitignoredTestLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        let logs = LOGS.get_or_init(|| Mutex::new(Vec::new()));
        let mut guard = logs.lock().expect("log mutex");
        guard.push(record.args().to_string());
    }

    fn flush(&self) {}
}

fn take_logs() -> (LoggerState, Vec<String>) {
    let state = *LOGGER_STATE.get_or_init(|| {
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(LevelFilter::Warn);
            LoggerState::TestLogger
        } else {
            LoggerState::OtherLogger
        }
    });
    let logs = LOGS.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = logs.lock().expect("log mutex");
    (state, guard.drain(..).collect())
}

#[test]
fn sync_ralph_state_copies_allowlisted_env_files_but_skips_ignored_dirs() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(
        repo_root.join(".gitignore"),
        ".env\n.env.local\ntarget/\n.ralph/cache/parallel/\n",
    )?;
    fs::write(repo_root.join(".env"), "secret")?;
    fs::write(repo_root.join(".env.local"), "local_secret")?;
    fs::create_dir_all(repo_root.join("target"))?;
    fs::write(
        repo_root.join("target/very_large_file.txt"),
        "heavy build output",
    )?;
    fs::create_dir_all(repo_root.join(".ralph/cache/parallel"))?;
    fs::write(
        repo_root.join(".ralph/cache/parallel/state.json"),
        "{\"cached\": true}",
    )?;

    let resolved = build_test_resolved(&repo_root, None, None);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(fs::read_to_string(workspace_root.join(".env"))?, "secret");
    assert_eq!(
        fs::read_to_string(workspace_root.join(".env.local"))?,
        "local_secret"
    );
    assert!(!workspace_root.join("target").exists());
    assert!(!workspace_root.join(".ralph/cache/parallel").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_copies_allowlisted_ignored_file() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(
        repo_root.join(".gitignore"),
        "local-tool.json\nunlisted.json\n",
    )?;
    fs::write(repo_root.join("local-tool.json"), "tool config")?;
    fs::write(repo_root.join("unlisted.json"), "skip me")?;

    let resolved = build_test_resolved_with_ignored_allowlist(&repo_root, vec!["local-tool.json"]);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("local-tool.json"))?,
        "tool config"
    );
    assert!(!workspace_root.join("unlisted.json").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_copies_allowlisted_ignored_glob_matches() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(repo_root.join(".gitignore"), "fixtures/*.json\n")?;
    fs::create_dir_all(repo_root.join("fixtures"))?;
    fs::write(repo_root.join("fixtures/local-a.json"), "a")?;
    fs::write(repo_root.join("fixtures/local-b.json"), "b")?;
    fs::write(repo_root.join("fixtures/other.json"), "other")?;

    let resolved =
        build_test_resolved_with_ignored_allowlist(&repo_root, vec!["fixtures/local-*.json"]);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("fixtures/local-a.json"))?,
        "a"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join("fixtures/local-b.json"))?,
        "b"
    );
    assert!(!workspace_root.join("fixtures/other.json").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_copies_allowlisted_file_under_ignored_directory_root() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(repo_root.join(".gitignore"), "config/\n")?;
    fs::create_dir_all(repo_root.join("config"))?;
    fs::write(repo_root.join("config/local.json"), "local config")?;
    fs::write(repo_root.join("config/other.json"), "do not copy")?;

    let resolved =
        build_test_resolved_with_ignored_allowlist(&repo_root, vec!["config/local.json"]);

    sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)?;
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("config/local.json"))?,
        "local config"
    );
    assert!(!workspace_root.join("config/other.json").exists());
    Ok(())
}

#[test]
fn sync_ralph_state_copies_allowlisted_glob_under_ignored_directory_root() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(repo_root.join(".gitignore"), "config/\n")?;
    fs::create_dir_all(repo_root.join("config"))?;
    fs::write(repo_root.join("config/local-a.json"), "a")?;
    fs::write(repo_root.join("config/local-b.json"), "b")?;
    fs::write(repo_root.join("config/other.json"), "other")?;

    let resolved =
        build_test_resolved_with_ignored_allowlist(&repo_root, vec!["config/local-*.json"]);

    sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)?;
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("config/local-a.json"))?,
        "a"
    );
    assert_eq!(
        fs::read_to_string(workspace_root.join("config/local-b.json"))?,
        "b"
    );
    assert!(!workspace_root.join("config/other.json").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn preflight_parallel_ignored_file_allowlist_rejects_external_symlink() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    let outside = temp.path().join("outside-secret.txt");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;
    fs::write(&outside, "do not copy")?;
    fs::write(repo_root.join(".gitignore"), "secret-link\n")?;
    unix_fs::symlink(&outside, repo_root.join("secret-link"))?;

    let resolved = build_test_resolved_with_ignored_allowlist(&repo_root, vec!["secret-link"]);
    let err =
        sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)
            .expect_err("expected external symlink to be rejected");

    let msg = err.to_string();
    assert!(msg.contains("resolves outside repo root"), "{msg}");
    assert!(msg.contains("secret-link"), "{msg}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn sync_ralph_state_rejects_external_ignored_symlink_without_copying() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    let outside = temp.path().join("outside-secret.txt");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;
    fs::write(&outside, "do not copy")?;
    fs::write(repo_root.join(".gitignore"), "secret-link\n")?;
    unix_fs::symlink(&outside, repo_root.join("secret-link"))?;

    let resolved = build_test_resolved_with_ignored_allowlist(&repo_root, vec!["secret-link"]);
    let err = sync_ralph_state(&resolved, &workspace_root)
        .expect_err("expected runtime sync to reject external symlink");

    let msg = err.to_string();
    assert!(msg.contains("resolves outside repo root"), "{msg}");
    assert!(msg.contains("secret-link"), "{msg}");
    assert!(!workspace_root.join("secret-link").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn sync_ralph_state_allows_ignored_symlink_resolving_inside_repo() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;
    fs::create_dir_all(repo_root.join("private"))?;
    fs::write(repo_root.join("private/source.txt"), "repo local")?;
    fs::write(repo_root.join(".gitignore"), "safe-link\n")?;
    unix_fs::symlink(
        repo_root.join("private/source.txt"),
        repo_root.join("safe-link"),
    )?;

    let resolved = build_test_resolved_with_ignored_allowlist(&repo_root, vec!["safe-link"]);
    sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)?;
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("safe-link"))?,
        "repo local"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn preflight_parallel_ignored_file_allowlist_rejects_symlink_resolving_to_denied_tree() -> Result<()>
{
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;
    fs::create_dir_all(repo_root.join("target"))?;
    fs::write(repo_root.join("target/source.txt"), "build artifact")?;
    fs::write(repo_root.join(".gitignore"), "safe-link\n")?;
    unix_fs::symlink(
        repo_root.join("target/source.txt"),
        repo_root.join("safe-link"),
    )?;

    let resolved = build_test_resolved_with_ignored_allowlist(&repo_root, vec!["safe-link"]);
    let err =
        sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)
            .expect_err("expected symlink resolving to denied tree to be rejected");

    let msg = err.to_string();
    assert!(msg.contains("denied runtime/build path"), "{msg}");
    assert!(msg.contains("safe-link"), "{msg}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn sync_ralph_state_rejects_symlink_resolving_inside_workspace_root() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = repo_root.join("workers/RQ-0001");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;
    fs::write(workspace_root.join("local.json"), "workspace artifact")?;
    fs::write(repo_root.join(".gitignore"), "safe-link\nworkers/\n")?;
    unix_fs::symlink(
        workspace_root.join("local.json"),
        repo_root.join("safe-link"),
    )?;

    let resolved = build_test_resolved_with_ignored_allowlist(&repo_root, vec!["safe-link"]);
    let err = sync_ralph_state(&resolved, &workspace_root)
        .expect_err("expected symlink resolving into workspace root to be rejected");

    let msg = err.to_string();
    assert!(msg.contains("parallel workspace root"), "{msg}");
    assert!(msg.contains("safe-link"), "{msg}");
    assert!(!workspace_root.join("safe-link").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn sync_ralph_state_rejects_default_env_symlink_resolving_outside_repo() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    let outside = temp.path().join("outside-env.txt");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;
    fs::write(&outside, "outside env")?;
    fs::write(repo_root.join(".gitignore"), ".env\n")?;
    unix_fs::symlink(&outside, repo_root.join(".env"))?;

    let resolved = build_test_resolved(&repo_root, None, None);
    let err = sync_ralph_state(&resolved, &workspace_root)
        .expect_err("expected default env symlink to be rejected");

    let msg = err.to_string();
    assert!(msg.contains("resolves outside repo root"), "{msg}");
    assert!(msg.contains(".env"), "{msg}");
    assert!(!workspace_root.join(".env").exists());
    Ok(())
}

#[test]
#[serial]
fn preflight_parallel_ignored_file_allowlist_warns_and_skips_missing_matches() -> Result<()> {
    let (logger_state, _) = take_logs();
    let _ = take_logs();

    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    let resolved =
        build_test_resolved_with_ignored_allowlist(&repo_root, vec!["missing.local.json"]);

    sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)?;

    let (_, logs) = take_logs();
    if logger_state == LoggerState::TestLogger {
        let joined = logs.join("\n");
        assert!(
            joined.contains("parallel.ignored_file_allowlist[0]"),
            "{joined}"
        );
        assert!(joined.contains("missing.local.json"), "{joined}");
        assert!(
            joined.contains("matched no existing gitignored files"),
            "{joined}"
        );
        assert!(joined.contains("skipping"), "{joined}");
    }
    Ok(())
}

#[test]
fn sync_ralph_state_skips_missing_allowlist_entry_but_copies_matching_entry() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;

    fs::write(repo_root.join(".gitignore"), "config/\n")?;
    fs::create_dir_all(repo_root.join("config"))?;
    fs::write(repo_root.join("config/local.json"), "local config")?;

    let resolved = build_test_resolved_with_ignored_allowlist(
        &repo_root,
        vec!["config/missing-*.json", "config/local.json"],
    );

    sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)?;
    sync_ralph_state(&resolved, &workspace_root)?;

    assert_eq!(
        fs::read_to_string(workspace_root.join("config/local.json"))?,
        "local config"
    );
    assert!(!workspace_root.join("config/missing-a.json").exists());
    Ok(())
}

#[test]
fn preflight_parallel_ignored_file_allowlist_rejects_broad_glob_matching_denied_path() -> Result<()>
{
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = temp.path().join("workspace");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::create_dir_all(&workspace_root)?;
    fs::write(repo_root.join(".gitignore"), "target/\n")?;
    fs::create_dir_all(repo_root.join("target"))?;
    fs::write(repo_root.join("target/local.json"), "build artifact")?;

    let resolved = build_test_resolved_with_ignored_allowlist(&repo_root, vec!["*/local.json"]);
    let err =
        sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)
            .expect_err("expected broad glob matching denied ignored path to fail");

    let msg = err.to_string();
    assert!(msg.contains("denied runtime/build path"), "{msg}");
    assert!(msg.contains("target/local.json"), "{msg}");
    Ok(())
}

#[test]
fn preflight_parallel_ignored_file_allowlist_rejects_workspace_descendants() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = repo_root.join(".ralph/workspaces/RQ-0001");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::write(repo_root.join(".gitignore"), ".ralph/workspaces/\n")?;
    fs::create_dir_all(&workspace_root)?;
    fs::write(workspace_root.join("local.json"), "workspace artifact")?;

    let resolved = build_test_resolved_with_ignored_allowlist(
        &repo_root,
        vec![".ralph/workspaces/RQ-0001/local.json"],
    );
    let err =
        sync_gitignored_impl::preflight_parallel_ignored_file_allowlist(&resolved, &workspace_root)
            .expect_err("expected workspace descendant to fail");

    assert!(
        err.to_string().contains("denied runtime/build path")
            || err.to_string().contains("workspace root")
    );
    Ok(())
}

#[test]
fn sync_ralph_state_skips_parent_of_workspace() -> Result<()> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let workspace_root = repo_root.join(".ralph/workspaces/RQ-0001");
    fs::create_dir_all(&repo_root)?;
    git_test::init_repo(&repo_root)?;
    fs::write(repo_root.join(".gitignore"), ".ralph/workspaces/\n")?;
    fs::create_dir_all(repo_root.join(".ralph/workspaces"))?;
    fs::write(
        repo_root.join(".ralph/workspaces/shared.txt"),
        "shared ignored",
    )?;
    fs::create_dir_all(&workspace_root)?;

    let resolved = build_test_resolved(&repo_root, None, None);
    sync_ralph_state(&resolved, &workspace_root)?;

    assert!(!workspace_root.join(".ralph/workspaces/shared.txt").exists());
    Ok(())
}

#[test]
fn should_sync_gitignored_entry_skips_empty() {
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(""));
}

#[test]
fn should_sync_gitignored_entry_skips_directories() {
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "target/"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "ignored_dir/"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "node_modules/"
    ));
}

#[test]
fn should_sync_gitignored_entry_allows_env_files() {
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(".env"));
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(
        ".env.local"
    ));
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(
        ".env.production"
    ));
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(
        ".env.development"
    ));
}

#[test]
fn should_sync_gitignored_entry_allows_nested_env_files() {
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(
        "nested/.env"
    ));
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(
        "nested/.env.production"
    ));
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(
        "config/.env.local"
    ));
}

#[test]
fn should_sync_gitignored_entry_skips_non_env_files() {
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "not_env.txt"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "README.md"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "secret.key"
    ));
}

#[test]
fn should_sync_gitignored_entry_skips_never_copy_prefixes() {
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "target/debug/app"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "target/release/lib.rlib"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "node_modules/lodash/index.js"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".venv/bin/python"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".ralph/cache/parallel/state.json"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".ralph/cache/plans/RQ-0001.md"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".ralph/workspaces/RQ-0001/.env"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".ralph/logs/run.log"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".ralph/lock/sync.lock"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "__pycache__/module.cpython-311.pyc"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".ruff_cache/0.1.0/content"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".pytest_cache/v/cache/nodeids"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".ty_cache/some_file"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".git/config"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        ".git/objects/abc"
    ));
}

#[test]
fn should_sync_gitignored_entry_with_allowlist_allows_configured_files() -> Result<()> {
    assert!(
        sync_gitignored_impl::should_sync_gitignored_entry_with_allowlist(
            "local/tool.json",
            &["local/*.json".to_string()]
        )?
    );
    assert!(
        !sync_gitignored_impl::should_sync_gitignored_entry_with_allowlist(
            "local/tool.toml",
            &["local/*.json".to_string()]
        )?
    );
    let err = sync_gitignored_impl::should_sync_gitignored_entry_with_allowlist(
        "target/local.json",
        &["target/*.json".to_string()],
    )
    .expect_err("denylisted allowlist entries should fail validation");
    assert!(err.to_string().contains("denied runtime/build path"));
    Ok(())
}

#[test]
fn should_sync_gitignored_entry_normalizes_leading_dot_slash() {
    assert!(sync_gitignored_impl::should_sync_gitignored_entry("./.env"));
    assert!(sync_gitignored_impl::should_sync_gitignored_entry(
        "./.env.local"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "./target/debug/app"
    ));
    assert!(!sync_gitignored_impl::should_sync_gitignored_entry(
        "./node_modules/lodash"
    ));
}
