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

pub mod branch;
pub mod clean;
pub mod commit;
pub mod error;
pub mod lfs;
pub mod pr;
pub mod status;
pub mod workspace;

// Re-export commonly used items for convenience within the crate.
pub(crate) use branch::current_branch;
pub use clean::{
    RALPH_RUN_CLEAN_ALLOWED_PATHS, repo_dirty_only_allowed_paths, require_clean_repo_ignoring_paths,
};
pub use commit::{
    add_paths_force, commit_all, is_ahead_of_upstream, push_upstream, push_upstream_allow_create,
    restore_tracked_paths_to_head, revert_uncommitted, upstream_ref,
};
pub use error::GitError;
pub use lfs::{check_lfs_health, filter_modified_lfs_files, has_lfs, list_lfs_files};
pub(crate) use pr::{
    MergeState, PrInfo, PrLifecycle, check_gh_available, create_pr, merge_pr, pr_lifecycle_status,
    pr_merge_status,
};
pub use status::{
    ensure_paths_unchanged, ignored_paths, is_path_ignored, snapshot_paths, status_paths,
    status_porcelain,
};
// NEW: workspace-based isolation (clone workspaces).
pub(crate) use workspace::{
    WorkspaceSpec, create_workspace_at, ensure_workspace_exists, origin_urls, remove_workspace,
    workspace_root,
};
