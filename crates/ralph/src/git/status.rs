//! Git status and porcelain parsing operations.
//!
//! This module provides functions for parsing git status output, tracking path
//! snapshots, and ensuring files remain unchanged during operations.
//!
//! # Invariants
//! - Porcelain parsing must handle NUL-terminated format correctly
//! - Path snapshots are deterministic and comparable
//!
//! # What this does NOT handle
//! - Commit operations (see git/commit.rs)
//! - LFS validation (see git/lfs.rs)
//! - Repository cleanliness enforcement (see git/clean.rs)

use crate::git::error::{GitError, git_base_command};
use anyhow::{Context, Result, anyhow, bail};
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::Hasher;
use std::io::Read;
use std::path::Path;

/// A snapshot of a file path with a fingerprint for detecting changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSnapshot {
    pub path: String,
    fingerprint: Option<u64>,
}

/// Internal representation of a porcelain -z entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PorcelainZEntry {
    pub xy: String,
    pub old_path: Option<String>,
    pub path: String,
}

/// Returns raw `git status --porcelain -z` output (may be empty).
///
/// NOTE: With `-z`, records are NUL-terminated (0x00) instead of newline-terminated.
/// This makes the output safe to parse even when filenames contain spaces/newlines.
pub fn status_porcelain(repo_root: &Path) -> Result<String, GitError> {
    let output = git_base_command(repo_root)
        .arg("status")
        .arg("--porcelain")
        .arg("-z")
        .output()
        .with_context(|| format!("run git status --porcelain -z in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(GitError::CommandFailed {
            args: "status --porcelain -z".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Returns a list of paths from git status.
pub fn status_paths(repo_root: &Path) -> Result<Vec<String>, GitError> {
    let status = status_porcelain(repo_root)?;
    if status.is_empty() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    let entries = parse_porcelain_z_entries(&status)?;
    for entry in entries {
        if !entry.path.is_empty() {
            paths.push(entry.path);
        }
    }
    Ok(paths)
}

/// Create deterministic fingerprints for a list of baseline dirty paths.
///
/// This is used to ensure Phase 1 plan-only runs do not mutate pre-existing
/// dirty files when `allow_dirty_repo` is true.
pub fn snapshot_paths(repo_root: &Path, paths: &[String]) -> Result<Vec<PathSnapshot>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut unique = HashSet::new();
    let mut snapshots = Vec::new();
    for path in paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.strip_prefix("./").unwrap_or(trimmed);
        if !unique.insert(normalized.to_string()) {
            continue;
        }
        let fingerprint = snapshot_path(&repo_root.join(normalized))?;
        snapshots.push(PathSnapshot {
            path: normalized.to_string(),
            fingerprint,
        });
    }

    snapshots.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(snapshots)
}

/// Validate that each baseline dirty path is unchanged from its fingerprint.
pub fn ensure_paths_unchanged(repo_root: &Path, snapshots: &[PathSnapshot]) -> Result<()> {
    for snapshot in snapshots {
        let current = snapshot_path(&repo_root.join(&snapshot.path))?;
        if current != snapshot.fingerprint {
            bail!(
                "Baseline dirty path changed during Phase 1: {}",
                snapshot.path
            );
        }
    }
    Ok(())
}

fn snapshot_path(path: &Path) -> Result<Option<u64>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        Ok(Some(hash_dir(path)?))
    } else if metadata.is_file() {
        Ok(Some(hash_file(path)?))
    } else if metadata.file_type().is_symlink() {
        let target = fs::read_link(path)?;
        Ok(Some(hash_bytes(&target.to_string_lossy())))
    } else {
        Ok(Some(metadata.len()))
    }
}

fn hash_dir(path: &Path) -> Result<u64> {
    let mut entries: Vec<_> = fs::read_dir(path)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut hasher = DefaultHasher::new();
    for entry in entries {
        let name = entry.file_name();
        hasher.write(name.to_string_lossy().as_bytes());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            hasher.write_u8(1);
            hasher.write_u64(hash_dir(&entry.path())?);
        } else if file_type.is_file() {
            hasher.write_u8(2);
            hasher.write_u64(hash_file(&entry.path())?);
        } else if file_type.is_symlink() {
            hasher.write_u8(3);
            let target = fs::read_link(entry.path())?;
            hasher.write(target.to_string_lossy().as_bytes());
        } else {
            hasher.write_u8(4);
            hasher.write_u64(entry.metadata()?.len());
        }
    }
    Ok(hasher.finish())
}

fn hash_file(path: &Path) -> Result<u64> {
    let mut file = fs::File::open(path)?;
    let mut hasher = DefaultHasher::new();
    let mut buf = [0u8; 8192];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.write(&buf[..read]);
    }
    Ok(hasher.finish())
}

fn hash_bytes(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write(value.as_bytes());
    hasher.finish()
}

/// Parse porcelain -z format entries from git status output.
pub(crate) fn parse_porcelain_z_entries(status: &str) -> Result<Vec<PorcelainZEntry>, GitError> {
    if status.is_empty() {
        return Ok(Vec::new());
    }

    // Keep simple split-based approach, but parse defensively:
    // - `git status --porcelain -z` is record-delimited by NUL
    // - trailing NULs (and even accidental consecutive NULs) should not truncate parsing
    let fields: Vec<&str> = status.split('\0').collect();
    let mut idx = 0usize;

    let mut entries = Vec::new();
    while let Some(entry) = parse_status_path(&fields, &mut idx)? {
        entries.push(entry);
    }
    Ok(entries)
}

fn is_rename_or_copy_xy(xy: &str) -> bool {
    let bytes = xy.as_bytes();
    if bytes.len() != 2 {
        return false;
    }
    matches!(bytes[0], b'R' | b'C') || matches!(bytes[1], b'R' | b'C')
}

fn take_required_field<'a>(
    fields: &'a [&'a str],
    idx: &mut usize,
    label: &str,
    head: &str,
    xy: &str,
) -> Result<&'a str, GitError> {
    let value = fields.get(*idx).copied().ok_or_else(|| {
        GitError::Other(anyhow!(
            "malformed porcelain -z output: missing {} after field {:?} (XY={:?}, next_index={})",
            label,
            head,
            xy,
            *idx
        ))
    })?;
    *idx = idx.saturating_add(1);

    if value.is_empty() {
        return Err(GitError::Other(anyhow!(
            "malformed porcelain -z output: empty {} after field {:?} (XY={:?})",
            label,
            head,
            xy
        )));
    }

    Ok(value)
}

fn parse_status_path(
    fields: &[&str],
    idx: &mut usize,
) -> Result<Option<PorcelainZEntry>, GitError> {
    // Skip empty fields so we don't prematurely stop on trailing NULs or accidental
    // consecutive NULs. This is defensive; valid git output should not include empty
    // records.
    while *idx < fields.len() && fields[*idx].is_empty() {
        *idx += 1;
    }

    if *idx >= fields.len() {
        return Ok(None);
    }

    let head = fields[*idx];
    *idx += 1;

    let (xy, inline_path) = parse_xy_and_inline_path(head)?;
    let is_rename_or_copy = is_rename_or_copy_xy(xy);

    let path = match inline_path {
        Some(path) => path,
        None => take_required_field(fields, idx, "path", head, xy)?,
    };

    if path.is_empty() {
        return Err(GitError::Other(anyhow!(
            "malformed porcelain -z output: empty path in field {:?} (XY={:?})",
            head,
            xy
        )));
    }

    let old_path = if is_rename_or_copy {
        Some(
            take_required_field(fields, idx, "old path field for rename/copy", head, xy)?
                .to_string(),
        )
    } else {
        None
    };

    Ok(Some(PorcelainZEntry {
        xy: xy.to_string(),
        old_path,
        path: path.to_string(),
    }))
}

fn parse_xy_and_inline_path(field: &str) -> Result<(&str, Option<&str>), GitError> {
    if field.len() < 2 {
        return Err(GitError::Other(anyhow!(
            "malformed porcelain -z output: field too short for XY status: {:?}",
            field
        )));
    }

    let xy = &field[..2];

    if field.len() == 2 {
        return Ok((xy, None));
    }

    let bytes = field.as_bytes();
    if bytes.len() >= 3 && bytes[2] == b' ' {
        return Ok((xy, Some(&field[3..])));
    }

    Err(GitError::Other(anyhow!(
        "malformed porcelain -z output: expected `XY<space>path` or `XY` field, got: {:?}",
        field
    )))
}

#[cfg(test)]
mod porcelain_parser_tests {
    use super::*;

    #[test]
    fn parse_porcelain_z_entries_skips_empty_fields_including_trailing_nuls() -> Result<()> {
        // The empty segment between two NULs should not truncate parsing.
        let status = "?? file1\0\0?? file2\0\0";
        let entries = parse_porcelain_z_entries(status)?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].xy, "??");
        assert_eq!(entries[0].path, "file1");
        assert_eq!(entries[1].xy, "??");
        assert_eq!(entries[1].path, "file2");
        Ok(())
    }

    #[test]
    fn parse_porcelain_z_entries_parses_copy_entries() -> Result<()> {
        // We unit-test C (copy) parsing directly rather than relying on git heuristics
        // to detect copies in a temp repo.
        let status = "C  new name.txt\0old name.txt\0";
        let entries = parse_porcelain_z_entries(status)?;
        assert_eq!(
            entries,
            vec![PorcelainZEntry {
                xy: "C ".to_string(),
                old_path: Some("old name.txt".to_string()),
                path: "new name.txt".to_string(),
            }]
        );
        Ok(())
    }
}
