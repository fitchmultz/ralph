//! Git helpers for repo status, commits, and LFS detection.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("repo is dirty; commit/stash your changes before running Ralph.{details}")]
    DirtyRepo { details: String },

    #[error("git {args} failed (code={code:?}): {stderr}")]
    CommandFailed {
        args: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("git push failed: no upstream configured for current branch. Set it with: git push -u origin <branch> OR git branch --set-upstream-to origin/<branch>.")]
    NoUpstream,

    #[error("git push failed: authentication/permission denied. Verify the remote URL, credentials, and that you have push access.")]
    AuthFailed,

    #[error("git push failed: {0}")]
    PushFailed(String),

    #[error("commit message is empty")]
    EmptyCommitMessage,

    #[error("no changes to commit")]
    NoChangesToCommit,

    #[error("no upstream configured for current branch")]
    NoUpstreamConfigured,

    #[error("unexpected rev-list output: {0}")]
    UnexpectedRevListOutput(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

fn classify_push_error(stderr: &str) -> GitError {
    let raw = stderr.trim();
    let lower = raw.to_lowercase();

    if lower.contains("no upstream")
        || lower.contains("set-upstream")
        || lower.contains("set the remote as upstream")
    {
        return GitError::NoUpstream;
    }

    if lower.contains("permission denied")
        || lower.contains("authentication failed")
        || lower.contains("access denied")
        || lower.contains("could not read from remote repository")
        || lower.contains("repository not found")
    {
        return GitError::AuthFailed;
    }

    let detail = if raw.is_empty() {
        "unknown git error".to_string()
    } else {
        raw.to_string()
    };
    GitError::PushFailed(detail)
}

// status_porcelain returns raw `git status --porcelain` output (may be empty).
pub fn status_porcelain(repo_root: &Path) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain")
        .output()
        .with_context(|| format!("run git status --porcelain in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(GitError::CommandFailed {
            args: "status --porcelain".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn status_paths(repo_root: &Path) -> Result<Vec<String>, GitError> {
    let status = status_porcelain(repo_root)?;
    if status.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for line in status.lines() {
        let trimmed = line.trim_start();
        if let Some(path) = parse_status_path(trimmed) {
            if !path.trim().is_empty() {
                paths.push(path.trim().to_string());
            }
        }
    }
    Ok(paths)
}

pub fn filter_modified_lfs_files(status_paths: &[String], lfs_files: &[String]) -> Vec<String> {
    if status_paths.is_empty() || lfs_files.is_empty() {
        return Vec::new();
    }

    let mut lfs_set = HashSet::new();
    for path in lfs_files {
        lfs_set.insert(path.trim().to_string());
    }

    let mut matches = Vec::new();
    for path in status_paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            continue;
        }
        if lfs_set.contains(trimmed) {
            matches.push(trimmed.to_string());
        }
    }

    matches.sort();
    matches.dedup();
    matches
}

pub fn require_clean_repo_ignoring_paths(
    repo_root: &Path,
    force: bool,
    allowed_paths: &[&str],
) -> Result<(), GitError> {
    let status = status_porcelain(repo_root)?;
    if status.trim().is_empty() {
        return Ok(());
    }

    if force {
        return Ok(());
    }

    let mut tracked = Vec::new();
    let mut untracked = Vec::new();

    for line in status.lines() {
        let trimmed = line.trim_start();
        let path = parse_status_path(trimmed).unwrap_or("");
        if !path_is_allowed(path, allowed_paths) {
            if trimmed.starts_with("??") {
                untracked.push(line);
            } else {
                tracked.push(line);
            }
        }
    }

    if tracked.is_empty() && untracked.is_empty() {
        return Ok(());
    }

    let mut details = String::new();

    if !tracked.is_empty() {
        details.push_str("\n\nTracked changes (suggest 'git stash' or 'git commit'):");
        for line in tracked.iter().take(10) {
            details.push_str("\n  ");
            details.push_str(line);
        }
        if tracked.len() > 10 {
            details.push_str(&format!("\n  ...and {} more", tracked.len() - 10));
        }
    }

    if !untracked.is_empty() {
        details.push_str("\n\nUntracked files (suggest 'git clean -fd' or 'git add'):");
        for line in untracked.iter().take(10) {
            details.push_str("\n  ");
            details.push_str(line);
        }
        if untracked.len() > 10 {
            details.push_str(&format!("\n  ...and {} more", untracked.len() - 10));
        }
    }

    details.push_str("\n\nUse --force to bypass this check if you are sure.");
    Err(GitError::DirtyRepo { details })
}

fn parse_status_path(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.len() < 3 {
        return None;
    }
    let status = &trimmed[..2];
    let rest = trimmed.get(2..)?.trim();
    if status == "??" {
        return Some(rest);
    }
    if let Some((_, path)) = rest.rsplit_once(" -> ") {
        return Some(path.trim());
    }
    Some(rest)
}

fn path_is_allowed(path: &str, allowed_paths: &[&str]) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.strip_prefix("./").unwrap_or(trimmed);
    allowed_paths.iter().any(|allowed| {
        let allowed_norm = allowed.strip_prefix("./").unwrap_or(allowed);
        normalized == allowed_norm
    })
}

fn git_run(repo_root: &Path, args: &[&str]) -> Result<(), GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Err(GitError::CommandFailed {
        args: args.join(" "),
        code: output.status.code(),
        stderr: stderr.trim().to_string(),
    })
}

// revert_uncommitted discards ONLY uncommitted changes.
// It does NOT reset to a pre-run SHA; it restores the working tree to current HEAD.
pub fn revert_uncommitted(repo_root: &Path) -> Result<(), GitError> {
    // Revert tracked changes in both index and working tree.
    // Prefer `git restore` (modern); fall back to older `git checkout` syntax.
    if git_run(repo_root, &["restore", "--staged", "--worktree", "."]).is_err() {
        // Older git fallback.
        git_run(repo_root, &["checkout", "--", "."]).context("fallback git checkout -- .")?;
        // Ensure staged changes are cleared too.
        git_run(repo_root, &["reset", "--quiet", "HEAD"]).context("git reset --quiet HEAD")?;
    }

    // Remove untracked files/directories created during the run.
    git_run(repo_root, &["clean", "-fd", "-e", ".env", "-e", ".env.*"])
        .context("git clean -fd -e .env*")?;
    Ok(())
}

// commit_all stages everything and creates a single commit.
pub fn commit_all(repo_root: &Path, message: &str) -> Result<(), GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::EmptyCommitMessage);
    }

    git_run(repo_root, &["add", "-A"]).context("git add -A")?;
    let status = status_porcelain(repo_root)?;
    if status.trim().is_empty() {
        return Err(GitError::NoChangesToCommit);
    }

    git_run(repo_root, &["commit", "-m", message]).context("git commit")?;
    Ok(())
}

// upstream_ref returns the configured upstream for the current branch (e.g. "origin/main").
pub fn upstream_ref(repo_root: &Path) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("--symbolic-full-name")
        .arg("@{u}")
        .output()
        .with_context(|| {
            format!(
                "run git rev-parse --abbrev-ref --symbolic-full-name @{{u}} in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(classify_push_error(&stderr));
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        return Err(GitError::NoUpstreamConfigured);
    }
    Ok(value)
}

// is_ahead_of_upstream reports whether HEAD is ahead of the configured upstream.
pub fn is_ahead_of_upstream(repo_root: &Path) -> Result<bool, GitError> {
    let upstream = upstream_ref(repo_root)?;
    let range = format!("{upstream}...HEAD");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-list")
        .arg("--left-right")
        .arg("--count")
        .arg(range)
        .output()
        .with_context(|| {
            format!(
                "run git rev-list --left-right --count in {}",
                repo_root.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(GitError::CommandFailed {
            args: "rev-list --left-right --count".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    let counts = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = counts.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(GitError::UnexpectedRevListOutput(counts.trim().to_string()));
    }

    let ahead: u32 = parts[1].parse().context("parse ahead count")?;
    Ok(ahead > 0)
}

// push_upstream pushes HEAD to the configured upstream.
pub fn push_upstream(repo_root: &Path) -> Result<(), GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("push")
        .output()
        .with_context(|| format!("run git push in {}", repo_root.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(classify_push_error(&stderr))
}

// has_lfs detects if Git LFS is initialized in the repository.
pub fn has_lfs(repo_root: &Path) -> Result<bool> {
    // Check for .git/lfs directory first
    let git_lfs_dir = repo_root.join(".git/lfs");
    if git_lfs_dir.is_dir() {
        return Ok(true);
    }

    // Check .gitattributes for LFS filter patterns
    let gitattributes = repo_root.join(".gitattributes");
    if gitattributes.is_file() {
        let content = fs::read_to_string(&gitattributes)
            .with_context(|| format!("read .gitattributes in {}", repo_root.display()))?;
        return Ok(content.contains("filter=lfs"));
    }

    Ok(false)
}

// list_lfs_files returns a list of LFS-tracked files in the repository.
pub fn list_lfs_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["lfs", "ls-files"])
        .output()
        .with_context(|| format!("run git lfs ls-files in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If LFS is not installed or initialized, return empty list
        if stderr.contains("not a git lfs repository")
            || stderr.contains("git: lfs is not a git command")
        {
            return Ok(Vec::new());
        }
        return Err(GitError::CommandFailed {
            args: "lfs ls-files".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        }
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    // Parse git lfs ls-files output format:
    // each line is: "SHA256 * path/to/file"
    for line in stdout.lines() {
        if let Some((_, path)) = line.rsplit_once(" * ") {
            files.push(path.to_string());
        }
    }

    Ok(files)
}
