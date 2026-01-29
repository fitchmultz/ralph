//! Git operations module.
//!
//! This module provides a comprehensive set of git operations for Ralph,
//! organized into focused submodules:
//!
//! - `error`: Error types and classification
//! - `status`: Status parsing and path tracking
//! - `lfs`: Git LFS detection and validation
//! - `commit`: Commit and push operations
//! - `clean`: Repository cleanliness validation
//!
//! # Invariants
//! - All operations use `-c core.fsmonitor=false` to avoid fsmonitor issues
//! - Error types are Send + Sync for anyhow compatibility
//! - LFS operations gracefully handle repositories without LFS
//!
//! # What this does NOT handle
//! - General file system operations (use std::fs or anyhow)
//! - Non-git version control systems

pub mod clean;
pub mod commit;
pub mod error;
pub mod lfs;
pub mod status;

// Re-export commonly used items for convenience
pub use clean::{
    repo_dirty_only_allowed_paths, require_clean_repo_ignoring_paths, RALPH_RUN_CLEAN_ALLOWED_PATHS,
};
pub use commit::{
    commit_all, is_ahead_of_upstream, push_upstream, revert_uncommitted, upstream_ref,
};
pub use error::{classify_push_error, GitError};
pub use lfs::{
    check_lfs_health, check_lfs_status, filter_modified_lfs_files, has_lfs, list_lfs_files,
    validate_lfs_filters, validate_lfs_pointers, LfsFilterStatus, LfsHealthReport, LfsPointerIssue,
    LfsStatusSummary,
};
pub use status::{
    ensure_paths_unchanged, snapshot_paths, status_paths, status_porcelain, PathSnapshot,
};
