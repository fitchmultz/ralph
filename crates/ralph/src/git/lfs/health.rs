//! Git LFS health reporting.
//!
//! Purpose:
//! - Git LFS health reporting.
//!
//! Responsibilities:
//! - Aggregate LFS detection, filter validation, status parsing, and pointer validation into one report.
//!
//! Not handled here:
//! - Low-level git command execution details beyond the delegated helpers.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Unexpected failures propagate only when LFS is detected and a sub-check fails.

use super::{
    check_lfs_status,
    detect::{has_lfs, list_lfs_files},
    filters::validate_lfs_filters,
    pointers::validate_lfs_pointers,
    types::LfsHealthReport,
};
use anyhow::Result;
use std::path::Path;

pub fn check_lfs_health(repo_root: &Path) -> Result<LfsHealthReport> {
    let lfs_initialized = has_lfs(repo_root)?;
    if !lfs_initialized {
        return Ok(LfsHealthReport {
            lfs_initialized: false,
            ..LfsHealthReport::default()
        });
    }

    let filter_status = Some(validate_lfs_filters(repo_root)?);
    let status_summary = Some(check_lfs_status(repo_root)?);
    let lfs_files = list_lfs_files(repo_root)?;
    let pointer_issues = if lfs_files.is_empty() {
        Vec::new()
    } else {
        validate_lfs_pointers(repo_root, &lfs_files)?
    };

    Ok(LfsHealthReport {
        lfs_initialized: true,
        filter_status,
        status_summary,
        pointer_issues,
    })
}
