//! Gitignored allowlist syncing for worker workspaces.
//!
//! Purpose:
//! - Gitignored allowlist syncing for worker workspaces.
//!
//! Responsibilities:
//! - Filter ignored repository paths through Ralph's canonical parallel-worker policy.
//! - Copy safe ignored files such as `.env*` plus trusted explicit allowlist matches.
//! - Validate configured ignored-file allowlist entries and fail fast on unsafe input.
//! - Avoid recursive self-copy when workspaces live under the repo root.
//!
//! Non-scope:
//! - `.ralph` runtime-tree traversal.
//! - Queue/done bookkeeping path seeding.
//! - Recursive directory allowlisting or broad ignored-file mirroring.
//!
//! Usage:
//! - Used by parallel workspace state sync and preflight validation.
//!
//! Invariants:
//! - Directories and heavyweight cache/build trees are never copied.
//! - Default automatic syncing remains limited to `.env` and `.env.*` basenames.
//! - Additional ignored files require explicit repo-relative file/glob allowlist entries.

use crate::config;
use crate::git;
use anyhow::{Context, Result, bail};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::common::sync_file_if_exists;

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

#[derive(Debug, Clone)]
struct AllowlistPattern {
    index: usize,
    raw: String,
    glob: GlobSet,
}

/// Validate `parallel.ignored_file_allowlist` entries without touching the repo.
pub(crate) fn validate_parallel_ignored_file_allowlist_config(entries: &[String]) -> Result<()> {
    for (index, raw) in entries.iter().enumerate() {
        let key = format!("parallel.ignored_file_allowlist[{index}]");
        normalize_allowlist_entry(raw, &key)?;
    }
    Ok(())
}

/// Validate configured ignored-file allowlist entries against the current repo state.
pub(crate) fn preflight_parallel_ignored_file_allowlist(
    resolved: &config::Resolved,
    workspace_root: &Path,
) -> Result<()> {
    let Some(entries) = resolved.config.parallel.ignored_file_allowlist.as_deref() else {
        return Ok(());
    };
    if entries.is_empty() {
        return Ok(());
    }

    let patterns = compile_allowlist_patterns(entries)?;
    let canonical_root = canonical_repo_root(&resolved.repo_root)?;
    let ignored = git::ignored_paths(&resolved.repo_root).with_context(|| {
        format!(
            "Parallel preflight: list ignored paths in {}",
            resolved.repo_root.display()
        )
    })?;
    let workspace_rel = workspace_relative(&resolved.repo_root, workspace_root);

    for pattern in &patterns {
        let mut matches = Vec::new();
        for raw_rel in &ignored {
            let rel = normalize_git_entry_for_matching(raw_rel);
            if rel.is_empty() {
                continue;
            }
            if !pattern.glob.is_match(&rel) {
                continue;
            }
            if let Some(prefix) = denied_prefix(&rel) {
                bail!(
                    "Parallel preflight: parallel.ignored_file_allowlist[{}] `{}` matched `{}`, which is under denied runtime/build path `{prefix}`. Remove or narrow the entry.",
                    pattern.index,
                    pattern.raw,
                    rel
                );
            }
            if workspace_rel_excludes(&rel, workspace_rel.as_deref()) {
                bail!(
                    "Parallel preflight: parallel.ignored_file_allowlist[{}] `{}` matched `{}`, which is inside the parallel workspace root. Remove the entry or move parallel.workspace_root outside the allowlisted path.",
                    pattern.index,
                    pattern.raw,
                    rel
                );
            }
            let context = format!(
                "Parallel preflight: parallel.ignored_file_allowlist[{}] `{}`",
                pattern.index, pattern.raw
            );
            if safe_ignored_file_source(
                &resolved.repo_root,
                &canonical_root,
                &rel,
                &context,
                workspace_rel.as_deref(),
            )?
            .is_some()
            {
                matches.push(rel);
            }
        }
        if matches.is_empty() {
            log::warn!(
                "Parallel preflight: parallel.ignored_file_allowlist[{}] `{}` matched no existing gitignored files; skipping optional ignored-file sync for this entry.",
                pattern.index,
                pattern.raw
            );
        }
    }

    Ok(())
}

#[cfg(test)]
pub(super) fn should_sync_gitignored_entry(raw_git_entry: &str) -> bool {
    let normalized = normalize_git_entry_for_matching(raw_git_entry);
    classify_gitignored_entry(&normalized, None).is_some()
}

#[cfg(test)]
pub(super) fn should_sync_gitignored_entry_with_allowlist(
    raw_git_entry: &str,
    entries: &[String],
) -> Result<bool> {
    let compiled = compile_allowlist(entries)?;
    let normalized = normalize_git_entry_for_matching(raw_git_entry);
    Ok(classify_gitignored_entry(&normalized, Some(&compiled)).is_some())
}

pub(super) fn sync_gitignored(resolved: &config::Resolved, workspace_path: &Path) -> Result<()> {
    let repo_root = &resolved.repo_root;
    let ignored = git::ignored_paths(repo_root)
        .with_context(|| format!("list ignored paths in {}", repo_root.display()))?;
    if ignored.is_empty() {
        return Ok(());
    }

    let allowlist = resolved.config.parallel.ignored_file_allowlist.as_deref();
    let compiled = allowlist.map(compile_allowlist).transpose()?;
    let workspace_rel = workspace_relative(repo_root, workspace_path);
    let canonical_root = canonical_repo_root(repo_root)?;

    for raw_rel in ignored {
        let rel = normalize_git_entry_for_matching(&raw_rel);
        if classify_gitignored_entry(&rel, compiled.as_ref()).is_none() {
            continue;
        }
        if workspace_rel_excludes(&rel, workspace_rel.as_deref()) {
            continue;
        }

        let Some(source) = safe_ignored_file_source(
            repo_root,
            &canonical_root,
            &rel,
            "Parallel ignored-file sync",
            workspace_rel.as_deref(),
        )?
        else {
            continue;
        };
        let target = workspace_path.join(&rel);

        sync_file_if_exists(&source, &target)?;
        log::debug!(
            "Parallel ignored-file sync: copied {} to {}",
            rel,
            target.display()
        );
    }

    Ok(())
}

fn canonical_repo_root(repo_root: &Path) -> Result<PathBuf> {
    repo_root
        .canonicalize()
        .with_context(|| format!("canonicalize repo root {}", repo_root.display()))
}

fn safe_ignored_file_source(
    repo_root: &Path,
    canonical_root: &Path,
    rel: &str,
    context: &str,
    workspace_rel: Option<&str>,
) -> Result<Option<PathBuf>> {
    let source = repo_root.join(rel);
    let canonical_source = match source.canonicalize() {
        Ok(path) => path,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "{context}: resolve ignored file `{rel}` at {}",
                    source.display()
                )
            });
        }
    };

    if !canonical_source.starts_with(canonical_root) {
        bail!(
            "{context}: ignored file `{rel}` resolves outside repo root (resolved: {}, repo: {}). Refusing to sync gitignored symlink or path.",
            canonical_source.display(),
            canonical_root.display()
        );
    }
    let canonical_rel = canonical_source
        .strip_prefix(canonical_root)
        .with_context(|| format!("{context}: relativize resolved ignored file `{rel}`"))?;
    let canonical_rel = canonical_rel
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    if let Some(prefix) = denied_prefix(&canonical_rel) {
        bail!(
            "{context}: ignored file `{rel}` resolves under denied runtime/build path `{prefix}` (resolved: {}). Refusing to sync gitignored symlink or path.",
            canonical_source.display()
        );
    }
    if workspace_rel_excludes(&canonical_rel, workspace_rel) {
        bail!(
            "{context}: ignored file `{rel}` resolves inside the parallel workspace root (resolved: {}). Refusing to sync gitignored symlink or path.",
            canonical_source.display()
        );
    }

    let metadata = fs::metadata(&source)
        .with_context(|| format!("{context}: inspect ignored file `{rel}`"))?;
    if metadata.is_dir() {
        bail!(
            "{context}: matched directory `{rel}`. Directories are not supported; use file paths or file globs."
        );
    }
    if metadata.is_file() {
        return Ok(Some(canonical_source));
    }

    Ok(None)
}

fn compile_allowlist(entries: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for (index, raw) in entries.iter().enumerate() {
        let key = format!("parallel.ignored_file_allowlist[{index}]");
        let normalized = normalize_allowlist_entry(raw, &key)?;
        let glob = GlobBuilder::new(&normalized)
            .literal_separator(true)
            .build()
            .with_context(|| format!("Invalid {key}: invalid glob pattern `{raw}`"))?;
        builder.add(glob);
    }
    builder
        .build()
        .context("compile parallel.ignored_file_allowlist")
}

fn compile_allowlist_patterns(entries: &[String]) -> Result<Vec<AllowlistPattern>> {
    let mut patterns = Vec::new();
    for (index, raw) in entries.iter().enumerate() {
        let key = format!("parallel.ignored_file_allowlist[{index}]");
        let normalized = normalize_allowlist_entry(raw, &key)?;
        let mut builder = GlobSetBuilder::new();
        let glob = GlobBuilder::new(&normalized)
            .literal_separator(true)
            .build()
            .with_context(|| format!("Invalid {key}: invalid glob pattern `{raw}`"))?;
        builder.add(glob);
        patterns.push(AllowlistPattern {
            index,
            raw: raw.clone(),
            glob: builder
                .build()
                .context("compile parallel.ignored_file_allowlist entry")?,
        });
    }
    Ok(patterns)
}

fn normalize_allowlist_entry(raw: &str, key: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("Invalid {key}: entries must not be empty");
    }
    let normalized = trimmed
        .strip_prefix("./")
        .unwrap_or(trimmed)
        .replace('\\', "/");
    if normalized.starts_with('/') || looks_like_windows_absolute(&normalized) {
        bail!("Invalid {key}: path must be repo-relative (got {trimmed})");
    }
    if normalized.ends_with('/') {
        bail!(
            "Invalid {key}: directories are not supported; use file paths or globs (got {trimmed})"
        );
    }
    if normalized.split('/').any(|part| part == "..") {
        bail!("Invalid {key}: path must not contain '..' components (got {trimmed})");
    }
    if normalized.split('/').any(|part| part == ".") {
        bail!("Invalid {key}: path must be normalized without '.' components (got {trimmed})");
    }
    if let Some(prefix) = denied_prefix(&normalized) {
        bail!("Invalid {key}: entry is under denied runtime/build path `{prefix}` (got {trimmed})");
    }
    Ok(normalized)
}

fn looks_like_windows_absolute(normalized: &str) -> bool {
    let bytes = normalized.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' && bytes[0].is_ascii_alphabetic()
}

fn normalize_git_entry_for_matching(raw_git_entry: &str) -> String {
    raw_git_entry
        .trim()
        .strip_prefix("./")
        .unwrap_or(raw_git_entry.trim())
        .replace('\\', "/")
}

fn classify_gitignored_entry(normalized: &str, allowlist: Option<&GlobSet>) -> Option<()> {
    if normalized.is_empty() || normalized.ends_with('/') || denied_prefix(normalized).is_some() {
        return None;
    }
    if default_env_allowed(normalized) {
        return Some(());
    }
    if allowlist.is_some_and(|set| set.is_match(normalized)) {
        return Some(());
    }
    None
}

fn default_env_allowed(normalized: &str) -> bool {
    let basename = Path::new(normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    basename == ".env" || basename.starts_with(".env.")
}

fn denied_prefix(normalized: &str) -> Option<&'static str> {
    NEVER_COPY_PREFIXES
        .iter()
        .copied()
        .find(|prefix| normalized.starts_with(prefix) || normalized.contains(prefix))
}

fn workspace_relative(repo_root: &Path, workspace_path: &Path) -> Option<String> {
    workspace_path.strip_prefix(repo_root).ok().map(|path| {
        path.to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
            .trim_end_matches('/')
            .to_string()
    })
}

fn workspace_rel_excludes(rel: &str, workspace_rel: Option<&str>) -> bool {
    let Some(prefix) = workspace_rel else {
        return false;
    };
    !prefix.is_empty()
        && (rel == prefix
            || rel.starts_with(&format!("{prefix}/"))
            || prefix.starts_with(&format!("{rel}/")))
}
