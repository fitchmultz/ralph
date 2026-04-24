//! Git LFS facade.
//!
//! Purpose:
//! - Git LFS facade.
//!
//! Responsibilities:
//! - Expose Git LFS detection, filter/status parsing, pointer validation, and health reporting.
//! - Keep LFS concerns split into focused submodules instead of one large implementation file.
//!
//! Not handled here:
//! - Regular git status/commit/clean operations.
//! - Repository cleanliness policy outside LFS.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Repositories without LFS remain a healthy no-op case.
//! - LFS pointer validation only applies to small text pointer files.

mod detect;
mod filters;
mod health;
mod pointers;
mod status;
mod types;

#[cfg(test)]
mod tests;

pub use detect::{has_lfs, list_lfs_files};
pub use health::check_lfs_health;
pub use pointers::filter_modified_lfs_files;
#[cfg(test)]
pub(crate) use pointers::validate_lfs_pointers;
pub(crate) use status::check_lfs_status;
pub use types::{LfsFilterStatus, LfsHealthReport, LfsPointerIssue, LfsStatusSummary};
