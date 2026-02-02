//! State synchronization and git helpers for parallel workers.
//!
//! Responsibilities:
//! - Sync ralph state (queue, done, config, prompts) to worker workspaces.
//! - Commit changes on worker failure for draft PR creation.
//! - Ensure branches are pushed before creating PRs.
//!
//! Not handled here:
//! - Worker lifecycle (see `super::worker`).
//! - PR creation or merge logic (see `super::merge_runner`).
//!
//! Invariants/assumptions:
//! - Source files exist in the main repo's `.ralph/` directory.
//! - Workspace paths are valid and writable.

use crate::git;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Sync ralph state files from repo root to workspace.
pub(crate) fn sync_ralph_state(repo_root: &Path, workspace_path: &Path) -> Result<()> {
    let source = repo_root.join(".ralph");
    let target = workspace_path.join(".ralph");
    fs::create_dir_all(&target)
        .with_context(|| format!("create workspace ralph dir {}", target.display()))?;

    sync_file_if_exists(&source.join("queue.json"), &target.join("queue.json"))?;
    sync_file_if_exists(&source.join("done.json"), &target.join("done.json"))?;
    sync_file_if_exists(&source.join("config.json"), &target.join("config.json"))?;
    sync_prompts_dir(&source.join("prompts"), &target.join("prompts"))?;
    sync_gitignored(repo_root, workspace_path)?;

    Ok(())
}

/// Commit any pending changes in the workspace after a failure.
/// Returns true if changes were committed, false if there were no changes.
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
pub(crate) fn ensure_branch_pushed(workspace_path: &Path) -> Result<()> {
    match git::is_ahead_of_upstream(workspace_path) {
        Ok(ahead) => {
            if !ahead {
                return Ok(());
            }
            git::push_upstream(workspace_path).with_context(|| "push branch to upstream")?;
            Ok(())
        }
        Err(git::GitError::NoUpstream) | Err(git::GitError::NoUpstreamConfigured) => {
            git::push_upstream_allow_create(workspace_path)
                .with_context(|| "push branch and create upstream")?;
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
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

fn sync_prompts_dir(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(target)
        .with_context(|| format!("create workspace prompts dir {}", target.display()))?;
    for entry in
        fs::read_dir(source).with_context(|| format!("read prompts dir {}", source.display()))?
    {
        let entry = entry.with_context(|| format!("read prompts entry in {}", source.display()))?;
        let path = entry.path();
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
            && let Some(name) = path.file_name()
        {
            let dest = target.join(name);
            fs::copy(&path, &dest)
                .with_context(|| format!("sync {} to {}", path.display(), dest.display()))?;
        }
    }
    Ok(())
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
        let rel_trimmed = rel.trim_end_matches('/');
        if rel_trimmed.is_empty() {
            continue;
        }
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
        let metadata = fs::symlink_metadata(&source)
            .with_context(|| format!("stat ignored path {}", source.display()))?;
        if metadata.is_dir() {
            sync_path_recursive(&source, &target)?;
        } else {
            sync_file_if_exists(&source, &target)?;
        }
    }

    Ok(())
}

fn sync_path_recursive(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)
        .with_context(|| format!("create ignored dir {}", target.display()))?;
    for entry in
        fs::read_dir(source).with_context(|| format!("read ignored dir {}", source.display()))?
    {
        let entry = entry.with_context(|| format!("read ignored entry in {}", source.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        let dest = target.join(name);
        let metadata = entry
            .metadata()
            .with_context(|| format!("stat ignored entry {}", path.display()))?;
        if metadata.is_dir() {
            sync_path_recursive(&path, &dest)?;
        } else {
            sync_file_if_exists(&path, &dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn sync_ralph_state_copies_queue_and_prompts() -> Result<()> {
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

        sync_ralph_state(&repo_root, &workspace_root)?;

        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/queue.json"))?,
            "{queue}"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/done.json"))?,
            "{done}"
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
    fn sync_ralph_state_copies_ignored_paths() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path().join("repo");
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&repo_root)?;
        git_test::init_repo(&repo_root)?;
        fs::create_dir_all(&workspace_root)?;
        fs::write(
            repo_root.join(".gitignore"),
            ".env\n.ralph/README.md\nignored_dir/\n",
        )?;
        fs::write(repo_root.join(".env"), "secret")?;
        fs::create_dir_all(repo_root.join(".ralph"))?;
        fs::write(repo_root.join(".ralph/README.md"), "ralph readme")?;
        fs::create_dir_all(repo_root.join("ignored_dir"))?;
        fs::write(repo_root.join("ignored_dir/file.txt"), "ignored content")?;

        sync_ralph_state(&repo_root, &workspace_root)?;

        assert_eq!(fs::read_to_string(workspace_root.join(".env"))?, "secret");
        assert_eq!(
            fs::read_to_string(workspace_root.join(".ralph/README.md"))?,
            "ralph readme"
        );
        assert_eq!(
            fs::read_to_string(workspace_root.join("ignored_dir/file.txt"))?,
            "ignored content"
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

        sync_ralph_state(&repo_root, &workspace_root)?;

        assert!(
            !workspace_root.join(".ralph/workspaces/shared.txt").exists(),
            "workspace should not copy ignored parent dir into itself"
        );
        Ok(())
    }
}
