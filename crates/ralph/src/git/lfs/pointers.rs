//! Git LFS pointer validation.
//!
//! Purpose:
//! - Git LFS pointer validation.
//!
//! Responsibilities:
//! - Validate expected LFS pointer files and filter modified-path lists to LFS-tracked files.
//!
//! Not handled here:
//! - LFS detection or aggregated health reporting.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Large files are assumed to be checked-out LFS content and skipped as valid.

use super::types::LfsPointerIssue;
use crate::constants::defaults::LFS_POINTER_PREFIX;
use crate::constants::limits::MAX_POINTER_SIZE;
use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub(crate) fn validate_lfs_pointers(
    repo_root: &Path,
    files: &[String],
) -> Result<Vec<LfsPointerIssue>> {
    let mut issues = Vec::new();

    for file_path in files {
        let full_path = repo_root.join(file_path);
        let metadata = match fs::metadata(&full_path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        if metadata.len() > MAX_POINTER_SIZE {
            continue;
        }

        let content = match fs::read_to_string(&full_path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let trimmed = content.trim();

        if trimmed.starts_with(LFS_POINTER_PREFIX) {
            continue;
        }

        if trimmed.contains("git-lfs") || trimmed.contains("sha256") {
            issues.push(LfsPointerIssue::Corrupted {
                path: file_path.clone(),
                content_preview: trimmed.chars().take(50).collect(),
            });
            continue;
        }

        if !trimmed.is_empty() {
            issues.push(LfsPointerIssue::InvalidPointer {
                path: file_path.clone(),
                reason: "File does not match LFS pointer format".to_string(),
            });
        }
    }

    Ok(issues)
}

pub fn filter_modified_lfs_files(status_paths: &[String], lfs_files: &[String]) -> Vec<String> {
    if status_paths.is_empty() || lfs_files.is_empty() {
        return Vec::new();
    }

    let lfs_set: HashSet<String> = lfs_files
        .iter()
        .map(|path| path.trim().to_string())
        .collect();
    let mut matches = status_paths
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty() && lfs_set.contains(*path))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    matches
}
