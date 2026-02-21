//! State synchronization and git helpers for parallel workers.
//!
//! Responsibilities:
//! - Sync repo-local runtime state into worker workspaces.
//! - Commit changes on worker failure when diagnostics are needed.
//! - Provide push helpers for workspace branch synchronization.
//!
//! Not handled here:
//! - Worker lifecycle (see `super::worker`).
//! - Coordinator orchestration (see `super::orchestration`).
//!
//! Invariants/assumptions:
//! - Queue/done are not synchronized to workers (coordinator-only).
//! - Workspace paths are valid and writable.

use crate::config;
use crate::git;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Sync ralph state files from repo root to workspace.
///
/// Syncs `.ralph/` runtime files plus gitignored allowlisted files.
/// Queue/done and ephemeral `.ralph` runtime paths are intentionally NOT synchronized
/// to prevent coordinator state leakage and merge conflicts in worker branches.
///
/// # Errors
/// Returns an error if:
/// - File operations fail
pub(crate) fn sync_ralph_state(resolved: &config::Resolved, workspace_path: &Path) -> Result<()> {
    // Create .ralph directory for worker runtime state (always needed)
    let target = workspace_path.join(".ralph");
    fs::create_dir_all(&target)
        .with_context(|| format!("create workspace ralph dir {}", target.display()))?;

    // Sync repo-local .ralph runtime tree (excluding coordinator-only and ephemeral paths)
    let source = resolved.repo_root.join(".ralph");
    sync_ralph_runtime_tree(resolved, &source, &target)?;

    // Sync selected non-.ralph ignored files (currently .env*)
    sync_gitignored(&resolved.repo_root, workspace_path)?;

    Ok(())
}

/// Commit any pending changes in the workspace after a failure.
/// Returns true if changes were committed, false if there were no changes.
#[allow(dead_code)]
pub(crate) fn commit_failure_changes(workspace_path: &Path, task_id: &str) -> Result<bool> {
    let status = git::status_porcelain(workspace_path)?;
    if status.trim().is_empty() {
        return Ok(false);
    }

    let message = format!("WIP: {} (failed run)", task_id);
    match git::commit_all(workspace_path, &message) {
        Ok(()) => Ok(true),
        Err(err) => match err {
            git::GitError::NoChangesToCommit => Ok(false),
            _ => Err(err.into()),
        },
    }
}

/// Ensure the current branch in the workspace is pushed to upstream.
#[allow(dead_code)]
pub(crate) fn ensure_branch_pushed(workspace_path: &Path) -> Result<()> {
    git::push_upstream_with_rebase(workspace_path)
        .with_context(|| "push branch to upstream (auto-rebase on rejection)")
}

fn sync_file_if_exists(source: &Path, target: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create workspace dir {}", parent.display()))?;
    }
    fs::copy(source, target)
        .with_context(|| format!("sync {} to {}", source.display(), target.display()))?;
    Ok(())
}

fn sync_ralph_runtime_tree(
    resolved: &config::Resolved,
    source_root: &Path,
    target_root: &Path,
) -> Result<()> {
    if !source_root.is_dir() {
        return Ok(());
    }

    sync_ralph_runtime_tree_recursive(resolved, source_root, source_root, target_root)
}

fn sync_ralph_runtime_tree_recursive(
    resolved: &config::Resolved,
    source_root: &Path,
    current_source_dir: &Path,
    target_root: &Path,
) -> Result<()> {
    for entry in fs::read_dir(current_source_dir)
        .with_context(|| format!("read ralph dir {}", current_source_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("read ralph entry in {}", current_source_dir.display()))?;
        let source_path = entry.path();

        if should_skip_ralph_runtime_path(resolved, source_root, &source_path) {
            continue;
        }

        let rel_path = source_path.strip_prefix(source_root).with_context(|| {
            format!(
                "derive relative path from {} to {}",
                source_root.display(),
                source_path.display()
            )
        })?;
        let target_path = target_root.join(rel_path);

        let file_type = entry
            .file_type()
            .with_context(|| format!("read type for {}", source_path.display()))?;
        if file_type.is_dir() {
            fs::create_dir_all(&target_path)
                .with_context(|| format!("create workspace dir {}", target_path.display()))?;
            sync_ralph_runtime_tree_recursive(resolved, source_root, &source_path, target_root)?;
            continue;
        }

        sync_file_if_exists(&source_path, &target_path)?;
    }

    Ok(())
}

fn should_skip_ralph_runtime_path(
    resolved: &config::Resolved,
    source_root: &Path,
    source_path: &Path,
) -> bool {
    const NEVER_COPY_RALPH_DIRS: &[&str] = &["cache", "workspaces", "logs", "lock"];

    let Ok(rel_path) = source_path.strip_prefix(source_root) else {
        return true;
    };

    if rel_path.components().count() == 1
        && let Some(name) = rel_path.file_name().and_then(|name| name.to_str())
        && matches!(
            name,
            "queue.json" | "queue.jsonc" | "done.json" | "done.jsonc"
        )
    {
        return true;
    }

    if source_path == resolved.queue_path || source_path == resolved.done_path {
        return true;
    }

    rel_path
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .is_some_and(|component| NEVER_COPY_RALPH_DIRS.contains(&component))
}

/// Decide whether a gitignored entry should be synced to workspaces.
///
/// Policy:
/// - Ignore empty entries
/// - Skip entries with trailing '/' (directories)
/// - Skip entries under never-copy prefixes (target/, node_modules/, .ralph/cache/, etc.)
/// - Allow only files whose basename is `.env` or starts with `.env.`
fn should_sync_gitignored_entry(raw_git_entry: &str) -> bool {
    // Never-copy directory prefixes/components
    const NEVER_COPY_PREFIXES: &[&str] = &[
        "target/",
        "node_modules/",
        ".venv/",
        ".ralph/cache/",
        ".ralph/workspaces/",
        ".ralph/logs/",
        ".ralph/lock/",
        "__pycache__/",
        ".ruff_cache/",
        ".pytest_cache/",
        ".ty_cache/",
        ".git/",
    ];

    if raw_git_entry.is_empty() {
        return false;
    }

    // Strip leading ./ if present
    let normalized = raw_git_entry.strip_prefix("./").unwrap_or(raw_git_entry);

    // Detect directory hint from git (trailing /)
    let is_dir_hint = normalized.ends_with('/');
    if is_dir_hint {
        return false;
    }

    let rel_trimmed = normalized;

    // Check never-copy prefixes
    for prefix in NEVER_COPY_PREFIXES {
        if rel_trimmed.starts_with(prefix) {
            return false;
        }
        // Also check if any path component matches a never-copy prefix (without trailing /)
        if rel_trimmed.contains(prefix) {
            return false;
        }
    }

    // Allow only .env files
    let basename = Path::new(rel_trimmed)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    basename == ".env" || basename.starts_with(".env.")
}

fn sync_gitignored(repo_root: &Path, workspace_path: &Path) -> Result<()> {
    let ignored = git::ignored_paths(repo_root)
        .with_context(|| format!("list ignored paths in {}", repo_root.display()))?;
    if ignored.is_empty() {
        return Ok(());
    }

    let workspace_rel = workspace_path.strip_prefix(repo_root).ok().map(|path| {
        path.to_string_lossy()
            .trim_end_matches(std::path::MAIN_SEPARATOR)
            .trim_end_matches('/')
            .to_string()
    });

    for rel in ignored {
        // Apply allow/deny policy first, before any filesystem operations
        if !should_sync_gitignored_entry(&rel) {
            continue;
        }

        let rel_trimmed = rel.trim_end_matches('/');
        if rel_trimmed.is_empty() {
            continue;
        }

        // Skip workspace self-copy (existing behavior preserved)
        if let Some(prefix) = &workspace_rel
            && (rel_trimmed == prefix
                || rel_trimmed.starts_with(&format!("{}/", prefix))
                || prefix.starts_with(&format!("{}/", rel_trimmed)))
        {
            continue;
        }

        let source = repo_root.join(rel_trimmed);
        let target = workspace_path.join(rel_trimmed);
        if !source.exists() {
            continue;
        }

        // Since we skip directories above, this should always be a file
        sync_file_if_exists(&source, &target)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Config;
    use crate::testsupport::git as git_test;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn build_test_resolved(
        repo_root: &Path,
        queue_path: Option<PathBuf>,
        done_path: Option<PathBuf>,
    ) -> config::Resolved {
        let queue_path = queue_path.unwrap_or_else(|| repo_root.join(".ralph/queue.json"));
        let done_path = done_path.unwrap_or_else(|| repo_root.join(".ralph/done.json"));
        config::Resolved {
            config: Config::default(),
            repo_root: repo_root.to_path_buf(),
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: None,
        }
    }

    #[test]
    fn sync_ralph_state_copies_config_and_prompts_without_queue_done() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&repo_root)?;
        git_test::init_repo(&repo_root)?;
        fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
        fs::create_dir_all(&workspace_root)?;
        fs::write(repo_root.join(".ralph/queue.json"), "{queue}")?;
        fs::write(repo_root.join(".ralph/done.json"), "{done}")?;
        fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
        fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;

        let resolved = build_test_resolved(&repo_root, None, None);
        sync_ralph_state(&resolved, &workspace_root)?;

        assert!(
            !workspace_root.join(".ralph/queue.json").exists(),
            "queue.json should not be synchronized to worker workspaces"
        );
        assert!(
            !workspace_root.join(".ralph/done.json").exists(),
            "done.json should not be synchronized to worker workspaces"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/config.json"))?,
            "{config}"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
            "prompt"
        );
        Ok(())
    }

    #[test]
    fn sync_ralph_state_copies_runtime_files_from_ignored_ralph_dir() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&repo_root)?;
        git_test::init_repo(&repo_root)?;
        fs::create_dir_all(&workspace_root)?;

        fs::write(repo_root.join(".gitignore"), ".ralph/\n")?;
        fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
        fs::create_dir_all(repo_root.join(".ralph/templates"))?;
        fs::create_dir_all(repo_root.join(".ralph/cache/parallel"))?;
        fs::create_dir_all(repo_root.join(".ralph/logs"))?;
        fs::create_dir_all(repo_root.join(".ralph/workspaces"))?;
        fs::create_dir_all(repo_root.join(".ralph/lock"))?;
        fs::write(repo_root.join(".ralph/queue.json"), "{queue}")?;
        fs::write(repo_root.join(".ralph/done.json"), "{done}")?;
        fs::write(
            repo_root.join(".ralph/config.jsonc"),
            "{/*comment*/\"version\":1}",
        )?;
        fs::write(
            repo_root.join(".ralph/prompts/worker.md"),
            "# Worker prompt",
        )?;
        fs::write(
            repo_root.join(".ralph/templates/task.json"),
            "{\"template\":true}",
        )?;
        fs::write(
            repo_root.join(".ralph/cache/parallel/state.json"),
            "{\"cached\":true}",
        )?;
        fs::write(repo_root.join(".ralph/logs/debug.log"), "debug")?;
        fs::write(
            repo_root.join(".ralph/workspaces/shared.txt"),
            "workspace state",
        )?;
        fs::write(repo_root.join(".ralph/lock/queue.lock"), "lock")?;

        let resolved = build_test_resolved(&repo_root, None, None);
        sync_ralph_state(&resolved, &workspace_root)?;

        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/config.jsonc"))?,
            "{/*comment*/\"version\":1}"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/prompts/worker.md"))?,
            "# Worker prompt"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/templates/task.json"))?,
            "{\"template\":true}"
        );
        assert!(
            !workspace_root.join(".ralph/queue.json").exists(),
            "queue.json should not be synchronized to worker workspaces"
        );
        assert!(
            !workspace_root.join(".ralph/done.json").exists(),
            "done.json should not be synchronized to worker workspaces"
        );
        assert!(
            !workspace_root.join(".ralph/cache").exists(),
            "cache/ should not be synchronized to worker workspaces"
        );
        assert!(
            !workspace_root.join(".ralph/logs").exists(),
            "logs/ should not be synchronized to worker workspaces"
        );
        assert!(
            !workspace_root.join(".ralph/workspaces").exists(),
            "workspaces/ should not be synchronized to worker workspaces"
        );
        assert!(
            !workspace_root.join(".ralph/lock").exists(),
            "lock/ should not be synchronized to worker workspaces"
        );

        Ok(())
    }

    #[test]
    fn sync_ralph_state_copies_allowlisted_env_files_but_skips_ignored_dirs() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&repo_root)?;
        git_test::init_repo(&repo_root)?;
        fs::create_dir_all(&workspace_root)?;
        // .gitignore ignores: .env files, target/ directory, .ralph/cache/parallel/
        fs::write(
            repo_root.join(".gitignore"),
            ".env\n.env.local\ntarget/\n.ralph/cache/parallel/\n",
        )?;
        // Create allowlisted env files
        fs::write(repo_root.join(".env"), "secret")?;
        fs::write(repo_root.join(".env.local"), "local_secret")?;
        // Create ignored directory that should NOT be synced
        fs::create_dir_all(repo_root.join("target"))?;
        fs::write(
            repo_root.join("target/very_large_file.txt"),
            "heavy build output",
        )?;
        // Create .ralph/cache/parallel/ directory that should NOT be synced
        fs::create_dir_all(repo_root.join(".ralph/cache/parallel"))?;
        fs::write(
            repo_root.join(".ralph/cache/parallel/state.json"),
            "{\"cached\": true}",
        )?;

        let resolved = build_test_resolved(&repo_root, None, None);
        sync_ralph_state(&resolved, &workspace_root)?;

        // Env files should be synced (allowlisted)
        assert_eq!(fs::read_to_string(workspace_root.join(".env"))?, "secret");
        assert_eq!(
            fs::read_to_string(workspace_root.join(".env.local"))?,
            "local_secret"
        );
        // Ignored directories should NOT be synced
        assert!(
            !workspace_root.join("target").exists(),
            "target/ directory should not be synced"
        );
        assert!(
            !workspace_root.join(".ralph/cache/parallel").exists(),
            ".ralph/cache/parallel/ directory should not be synced"
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

        assert!(
            !workspace_root.join(".ralph/workspaces/shared.txt").exists(),
            "workspace should not copy ignored parent dir into itself"
        );
        Ok(())
    }

    #[test]
    fn sync_ralph_state_custom_queue_done_paths_are_not_synced() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&repo_root)?;
        git_test::init_repo(&repo_root)?;

        // Create custom queue/done paths (non-default locations)
        let queue_path = repo_root.join("queue/active.json");
        let done_path = repo_root.join("archive/done.json");
        fs::create_dir_all(queue_path.parent().unwrap())?;
        fs::create_dir_all(done_path.parent().unwrap())?;
        fs::write(&queue_path, "{custom_queue}")?;
        fs::write(&done_path, "{custom_done}")?;

        // Create .ralph directory with config and prompts
        fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
        fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
        fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;

        fs::create_dir_all(&workspace_root)?;

        let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
        sync_ralph_state(&resolved, &workspace_root)?;

        // Verify custom paths are NOT synced
        assert!(
            !workspace_root.join("queue/active.json").exists(),
            "custom queue path should not be synchronized"
        );
        assert!(
            !workspace_root.join("archive/done.json").exists(),
            "custom done path should not be synchronized"
        );
        // Verify config and prompts still sync
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/config.json"))?,
            "{config}"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
            "prompt"
        );
        Ok(())
    }

    #[test]
    fn sync_ralph_state_custom_queue_done_paths_inside_ralph_are_not_synced() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&repo_root)?;
        git_test::init_repo(&repo_root)?;

        // Create custom queue/done paths under .ralph
        let queue_path = repo_root.join(".ralph/data/queue.jsonc");
        let done_path = repo_root.join(".ralph/data/done.json");
        fs::create_dir_all(queue_path.parent().unwrap())?;
        fs::write(&queue_path, "{custom_queue}")?;
        fs::write(&done_path, "{custom_done}")?;

        // Create .ralph runtime files that should still sync
        fs::create_dir_all(repo_root.join(".ralph/prompts"))?;
        fs::write(repo_root.join(".ralph/config.jsonc"), "{config}")?;
        fs::write(repo_root.join(".ralph/prompts/override.md"), "prompt")?;
        fs::create_dir_all(&workspace_root)?;

        let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
        sync_ralph_state(&resolved, &workspace_root)?;

        // Custom queue/done should be excluded even under .ralph
        assert!(
            !workspace_root.join(".ralph/data/queue.jsonc").exists(),
            "custom .ralph queue path should not be synchronized"
        );
        assert!(
            !workspace_root.join(".ralph/data/done.json").exists(),
            "custom .ralph done path should not be synchronized"
        );
        // Other runtime files should still sync
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/config.jsonc"))?,
            "{config}"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/prompts/override.md"))?,
            "prompt"
        );
        Ok(())
    }

    #[test]
    fn sync_ralph_state_missing_done_file_allowed() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&repo_root)?;
        git_test::init_repo(&repo_root)?;

        // Create queue but NOT done
        let queue_path = repo_root.join("queue/active.json");
        fs::create_dir_all(queue_path.parent().unwrap())?;
        fs::write(&queue_path, "{queue}")?;

        fs::create_dir_all(repo_root.join(".ralph"))?;
        fs::write(repo_root.join(".ralph/config.json"), "{config}")?;
        fs::create_dir_all(&workspace_root)?;

        let done_path = repo_root.join("archive/done.json");
        let resolved = build_test_resolved(&repo_root, Some(queue_path), Some(done_path));
        sync_ralph_state(&resolved, &workspace_root)?;

        // Queue should NOT be synchronized
        assert!(!workspace_root.join("queue/active.json").exists());
        // Done should NOT exist (wasn't created)
        assert!(!workspace_root.join("archive/done.json").exists());
        Ok(())
    }

    // Unit tests for should_sync_gitignored_entry filter
    #[test]
    fn should_sync_gitignored_entry_skips_empty() {
        assert!(!should_sync_gitignored_entry(""));
    }

    #[test]
    fn should_sync_gitignored_entry_skips_directories() {
        // Trailing / indicates directory from git
        assert!(!should_sync_gitignored_entry("target/"));
        assert!(!should_sync_gitignored_entry("ignored_dir/"));
        assert!(!should_sync_gitignored_entry("node_modules/"));
    }

    #[test]
    fn should_sync_gitignored_entry_allows_env_files() {
        assert!(should_sync_gitignored_entry(".env"));
        assert!(should_sync_gitignored_entry(".env.local"));
        assert!(should_sync_gitignored_entry(".env.production"));
        assert!(should_sync_gitignored_entry(".env.development"));
    }

    #[test]
    fn should_sync_gitignored_entry_allows_nested_env_files() {
        assert!(should_sync_gitignored_entry("nested/.env"));
        assert!(should_sync_gitignored_entry("nested/.env.production"));
        assert!(should_sync_gitignored_entry("config/.env.local"));
    }

    #[test]
    fn should_sync_gitignored_entry_skips_non_env_files() {
        assert!(!should_sync_gitignored_entry("not_env.txt"));
        assert!(!should_sync_gitignored_entry("README.md"));
        assert!(!should_sync_gitignored_entry("secret.key"));
    }

    #[test]
    fn should_sync_gitignored_entry_skips_never_copy_prefixes() {
        // target/
        assert!(!should_sync_gitignored_entry("target/debug/app"));
        assert!(!should_sync_gitignored_entry("target/release/lib.rlib"));
        // node_modules/
        assert!(!should_sync_gitignored_entry(
            "node_modules/lodash/index.js"
        ));
        // .venv/
        assert!(!should_sync_gitignored_entry(".venv/bin/python"));
        // .ralph/cache/
        assert!(!should_sync_gitignored_entry(
            ".ralph/cache/parallel/state.json"
        ));
        assert!(!should_sync_gitignored_entry(
            ".ralph/cache/plans/RQ-0001.md"
        ));
        // .ralph/workspaces/
        assert!(!should_sync_gitignored_entry(
            ".ralph/workspaces/RQ-0001/.env"
        ));
        // .ralph/logs/
        assert!(!should_sync_gitignored_entry(".ralph/logs/run.log"));
        // .ralph/lock/
        assert!(!should_sync_gitignored_entry(".ralph/lock/sync.lock"));
        // __pycache__/
        assert!(!should_sync_gitignored_entry(
            "__pycache__/module.cpython-311.pyc"
        ));
        // .ruff_cache/
        assert!(!should_sync_gitignored_entry(".ruff_cache/0.1.0/content"));
        // .pytest_cache/
        assert!(!should_sync_gitignored_entry(
            ".pytest_cache/v/cache/nodeids"
        ));
        // .ty_cache/
        assert!(!should_sync_gitignored_entry(".ty_cache/some_file"));
        // .git/
        assert!(!should_sync_gitignored_entry(".git/config"));
        assert!(!should_sync_gitignored_entry(".git/objects/abc"));
    }

    #[test]
    fn should_sync_gitignored_entry_normalizes_leading_dot_slash() {
        // ./.env should behave like .env
        assert!(should_sync_gitignored_entry("./.env"));
        assert!(should_sync_gitignored_entry("./.env.local"));
        // ./target/ should still be skipped
        assert!(!should_sync_gitignored_entry("./target/debug/app"));
        // ./node_modules/ should still be skipped
        assert!(!should_sync_gitignored_entry("./node_modules/lodash"));
    }
}
