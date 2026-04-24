//! Git LFS data types.
//!
//! Purpose:
//! - Git LFS data types.
//!
//! Responsibilities:
//! - Define LFS filter, status, pointer, and health-report models.
//! - Provide convenience helpers for health/issue reporting.
//!
//! Not handled here:
//! - Running git commands or parsing command output.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `lfs_initialized = false` means the repository should be treated as healthy.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsFilterStatus {
    pub smudge_installed: bool,
    pub clean_installed: bool,
    pub smudge_value: Option<String>,
    pub clean_value: Option<String>,
}

impl LfsFilterStatus {
    pub fn is_healthy(&self) -> bool {
        self.smudge_installed && self.clean_installed
    }

    pub fn issues(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if !self.smudge_installed {
            issues.push("LFS smudge filter not configured".to_string());
        }
        if !self.clean_installed {
            issues.push("LFS clean filter not configured".to_string());
        }
        issues
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LfsStatusSummary {
    pub staged_lfs: Vec<String>,
    pub staged_not_lfs: Vec<String>,
    pub unstaged_lfs: Vec<String>,
    pub untracked_attributes: Vec<String>,
}

impl LfsStatusSummary {
    pub fn is_clean(&self) -> bool {
        self.staged_not_lfs.is_empty()
            && self.untracked_attributes.is_empty()
            && self.unstaged_lfs.is_empty()
    }

    pub fn issue_descriptions(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if !self.staged_not_lfs.is_empty() {
            issues.push(format!(
                "Files staged as regular files but should be LFS: {}",
                self.staged_not_lfs.join(", ")
            ));
        }
        if !self.untracked_attributes.is_empty() {
            issues.push(format!(
                "Files match .gitattributes LFS patterns but are not tracked by LFS: {}",
                self.untracked_attributes.join(", ")
            ));
        }
        if !self.unstaged_lfs.is_empty() {
            issues.push(format!(
                "Modified LFS files not staged: {}",
                self.unstaged_lfs.join(", ")
            ));
        }
        issues
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LfsPointerIssue {
    InvalidPointer {
        path: String,
        reason: String,
    },
    BinaryContent {
        path: String,
    },
    Corrupted {
        path: String,
        content_preview: String,
    },
}

impl LfsPointerIssue {
    pub fn path(&self) -> &str {
        match self {
            Self::InvalidPointer { path, .. }
            | Self::BinaryContent { path }
            | Self::Corrupted { path, .. } => path,
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::InvalidPointer { path, reason } => {
                format!("Invalid LFS pointer for '{}': {}", path, reason)
            }
            Self::BinaryContent { path } => format!(
                "'{}' contains binary content but should be an LFS pointer (smudge filter may not be working)",
                path
            ),
            Self::Corrupted {
                path,
                content_preview,
            } => {
                format!(
                    "Corrupted LFS pointer for '{}': preview='{}'",
                    path, content_preview
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LfsHealthReport {
    pub lfs_initialized: bool,
    pub filter_status: Option<LfsFilterStatus>,
    pub status_summary: Option<LfsStatusSummary>,
    pub pointer_issues: Vec<LfsPointerIssue>,
}

impl LfsHealthReport {
    pub fn is_healthy(&self) -> bool {
        if !self.lfs_initialized {
            return true;
        }

        let Some(filter) = &self.filter_status else {
            return false;
        };
        if !filter.is_healthy() {
            return false;
        }

        let Some(status) = &self.status_summary else {
            return false;
        };
        if !status.is_clean() {
            return false;
        }

        self.pointer_issues.is_empty()
    }

    pub fn all_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if let Some(filter) = &self.filter_status {
            issues.extend(filter.issues());
        }
        if let Some(status) = &self.status_summary {
            issues.extend(status.issue_descriptions());
        }
        issues.extend(self.pointer_issues.iter().map(LfsPointerIssue::description));
        issues
    }
}
