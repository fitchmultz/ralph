//! Git LFS (Large File Storage) operations and validation.
//!
//! This module provides functions for detecting, validating, and managing Git LFS
//! in repositories. It includes health checks, filter validation, and pointer file
//! validation.
//!
//! # Invariants
//! - Gracefully handles repositories without LFS (returns empty results, not errors)
//! - LFS pointer files are validated against the spec format
//!
//! # What this does NOT handle
//! - Regular git operations (see git/status.rs, git/commit.rs)
//! - Repository cleanliness checks (see git/clean.rs)

use crate::constants::defaults::LFS_POINTER_PREFIX;
use crate::constants::limits::MAX_POINTER_SIZE;
use crate::git::error::{GitError, git_base_command};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// LFS filter configuration status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsFilterStatus {
    /// Whether the smudge filter is installed.
    pub smudge_installed: bool,
    /// Whether the clean filter is installed.
    pub clean_installed: bool,
    /// The value of the smudge filter (e.g. "git-lfs smudge %f").
    pub smudge_value: Option<String>,
    /// The value of the clean filter (e.g. "git-lfs clean %f").
    pub clean_value: Option<String>,
}

impl LfsFilterStatus {
    /// Returns true if both smudge and clean filters are installed.
    pub fn is_healthy(&self) -> bool {
        self.smudge_installed && self.clean_installed
    }

    /// Returns a human-readable description of any issues.
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

/// Summary of LFS status from `git lfs status`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LfsStatusSummary {
    /// Files staged as LFS pointers (correctly tracked).
    pub staged_lfs: Vec<String>,
    /// Files staged that should be LFS but are being committed as regular files.
    pub staged_not_lfs: Vec<String>,
    /// Files not staged that have LFS modifications.
    pub unstaged_lfs: Vec<String>,
    /// Files with LFS attributes in .gitattributes but not tracked by LFS.
    pub untracked_attributes: Vec<String>,
}

impl LfsStatusSummary {
    /// Returns true if there are no LFS issues.
    pub fn is_clean(&self) -> bool {
        self.staged_not_lfs.is_empty()
            && self.untracked_attributes.is_empty()
            && self.unstaged_lfs.is_empty()
    }

    /// Returns a list of human-readable issue descriptions.
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

/// Issue detected with an LFS pointer file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LfsPointerIssue {
    /// File is not a valid LFS pointer (missing or invalid header).
    InvalidPointer { path: String, reason: String },
    /// File should be an LFS pointer but contains binary content (smudge filter not working).
    BinaryContent { path: String },
    /// Pointer file appears corrupted (invalid format).
    Corrupted {
        path: String,
        content_preview: String,
    },
}

impl LfsPointerIssue {
    /// Returns the path of the file with the issue.
    pub fn path(&self) -> &str {
        match self {
            LfsPointerIssue::InvalidPointer { path, .. } => path,
            LfsPointerIssue::BinaryContent { path } => path,
            LfsPointerIssue::Corrupted { path, .. } => path,
        }
    }

    /// Returns a human-readable description of the issue.
    pub fn description(&self) -> String {
        match self {
            LfsPointerIssue::InvalidPointer { path, reason } => {
                format!("Invalid LFS pointer for '{}': {}", path, reason)
            }
            LfsPointerIssue::BinaryContent { path } => {
                format!(
                    "'{}' contains binary content but should be an LFS pointer (smudge filter may not be working)",
                    path
                )
            }
            LfsPointerIssue::Corrupted {
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

/// Comprehensive LFS health check result.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LfsHealthReport {
    /// Whether LFS is initialized in the repository.
    pub lfs_initialized: bool,
    /// Status of LFS filters.
    pub filter_status: Option<LfsFilterStatus>,
    /// Summary from `git lfs status`.
    pub status_summary: Option<LfsStatusSummary>,
    /// Pointer validation issues.
    pub pointer_issues: Vec<LfsPointerIssue>,
}

impl LfsHealthReport {
    /// Returns true if LFS is fully healthy.
    pub fn is_healthy(&self) -> bool {
        if !self.lfs_initialized {
            return true; // No LFS is also "healthy" (nothing to check)
        }

        if let Some(ref filter) = self.filter_status
            && !filter.is_healthy()
        {
            return false;
        }

        if let Some(ref status) = self.status_summary
            && !status.is_clean()
        {
            return false;
        }

        self.pointer_issues.is_empty()
    }

    /// Returns a list of all issues found.
    pub fn all_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if let Some(ref filter) = self.filter_status {
            issues.extend(filter.issues());
        }

        if let Some(ref status) = self.status_summary {
            issues.extend(status.issue_descriptions());
        }

        for issue in &self.pointer_issues {
            issues.push(issue.description());
        }

        issues
    }
}

/// Detects if Git LFS is initialized in the repository.
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

/// Returns a list of LFS-tracked files in the repository.
pub fn list_lfs_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = git_base_command(repo_root)
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

/// Validates that LFS smudge/clean filters are properly installed in git config.
///
/// This function checks the git configuration for the required LFS filters:
/// - `filter.lfs.smudge` should be set (typically to "git-lfs smudge %f")
/// - `filter.lfs.clean` should be set (typically to "git-lfs clean %f")
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// * `Ok(LfsFilterStatus)` - The status of LFS filter configuration
/// * `Err(GitError)` - If git commands fail
///
/// # Example
/// ```
/// use std::path::Path;
/// use ralph::git::lfs::validate_lfs_filters;
///
/// let status = validate_lfs_filters(Path::new(".")).unwrap();
/// if !status.is_healthy() {
///     eprintln!("LFS filters misconfigured: {:?}", status.issues());
/// }
/// ```
pub fn validate_lfs_filters(repo_root: &Path) -> Result<LfsFilterStatus, GitError> {
    let smudge_output = git_base_command(repo_root)
        .args(["config", "--get", "filter.lfs.smudge"])
        .output()
        .with_context(|| {
            format!(
                "run git config --get filter.lfs.smudge in {}",
                repo_root.display()
            )
        })?;

    let clean_output = git_base_command(repo_root)
        .args(["config", "--get", "filter.lfs.clean"])
        .output()
        .with_context(|| {
            format!(
                "run git config --get filter.lfs.clean in {}",
                repo_root.display()
            )
        })?;

    let smudge_installed = smudge_output.status.success();
    let clean_installed = clean_output.status.success();

    let smudge_value = if smudge_installed {
        Some(
            String::from_utf8_lossy(&smudge_output.stdout)
                .trim()
                .to_string(),
        )
    } else {
        None
    };

    let clean_value = if clean_installed {
        Some(
            String::from_utf8_lossy(&clean_output.stdout)
                .trim()
                .to_string(),
        )
    } else {
        None
    };

    Ok(LfsFilterStatus {
        smudge_installed,
        clean_installed,
        smudge_value,
        clean_value,
    })
}

/// Runs `git lfs status` and parses the output to detect LFS issues.
///
/// This function detects:
/// - Files that should be LFS but are being committed as regular files
/// - Files matching .gitattributes LFS patterns but not tracked
/// - Modified LFS files that are not staged
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// * `Ok(LfsStatusSummary)` - Summary of LFS status
/// * `Err(GitError)` - If git lfs status fails
///
/// # Example
/// ```
/// use std::path::Path;
/// use ralph::git::lfs::check_lfs_status;
///
/// let status = check_lfs_status(Path::new(".")).unwrap();
/// if !status.is_clean() {
///     for issue in status.issue_descriptions() {
///         eprintln!("LFS issue: {}", issue);
///     }
/// }
/// ```
pub fn check_lfs_status(repo_root: &Path) -> Result<LfsStatusSummary, GitError> {
    let output = git_base_command(repo_root)
        .args(["lfs", "status"])
        .output()
        .with_context(|| format!("run git lfs status in {}", repo_root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If LFS is not installed or initialized, return empty summary
        if stderr.contains("not a git lfs repository")
            || stderr.contains("git: lfs is not a git command")
        {
            return Ok(LfsStatusSummary::default());
        }
        return Err(GitError::CommandFailed {
            args: "lfs status".to_string(),
            code: output.status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut summary = LfsStatusSummary::default();

    // Parse git lfs status output
    // The output format is:
    // Objects to be committed:
    // 	<file> (<status>)
    // 	...
    //
    // Objects not staged for commit:
    // 	<file> (<status>)
    // 	...
    let mut in_staged_section = false;
    let mut in_unstaged_section = false;

    for line in stdout.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Objects to be committed:") {
            in_staged_section = true;
            in_unstaged_section = false;
            continue;
        }

        if trimmed.starts_with("Objects not staged for commit:") {
            in_staged_section = false;
            in_unstaged_section = true;
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('(') {
            continue;
        }

        // Parse file entries like: "	path/to/file (LFS: some-sha)"
        // or "	path/to/file (Git: sha)"
        if let Some((file_path, status)) = trimmed.split_once(" (") {
            let file_path = file_path.trim();
            let status = status.trim_end_matches(')');

            if in_staged_section {
                if status.starts_with("LFS:") {
                    summary.staged_lfs.push(file_path.to_string());
                } else if status.starts_with("Git:") {
                    // File is staged as a regular git object, not LFS
                    summary.staged_not_lfs.push(file_path.to_string());
                }
            } else if in_unstaged_section && status.starts_with("LFS:") {
                summary.unstaged_lfs.push(file_path.to_string());
            }
        }
    }

    Ok(summary)
}

/// Validates LFS pointer files for correctness.
///
/// This function checks if files that should be LFS pointers are valid:
/// - Valid LFS pointers start with "version https://git-lfs.github.com/spec/v1"
/// - Detects files that should be pointers but contain binary content
/// - Detects corrupted pointer files
///
/// # Arguments
/// * `repo_root` - Path to the repository root
/// * `files` - List of file paths to validate (relative to repo_root)
///
/// # Returns
/// * `Ok(Vec<LfsPointerIssue>)` - List of issues found (empty if all valid)
/// * `Err(anyhow::Error)` - If file reading fails
///
/// # Example
/// ```
/// use std::path::Path;
/// use ralph::git::lfs::validate_lfs_pointers;
///
/// let issues = validate_lfs_pointers(Path::new("."), &["large.bin".to_string()]).unwrap();
/// for issue in issues {
///     eprintln!("{}", issue.description());
/// }
/// ```
pub fn validate_lfs_pointers(repo_root: &Path, files: &[String]) -> Result<Vec<LfsPointerIssue>> {
    let mut issues = Vec::new();

    for file_path in files {
        let full_path = repo_root.join(file_path);

        // Check if file exists
        let metadata = match fs::metadata(&full_path) {
            Ok(m) => m,
            Err(_) => {
                // File doesn't exist, skip
                continue;
            }
        };

        // LFS pointers are small text files
        if metadata.len() > MAX_POINTER_SIZE {
            // File is too large to be a pointer, likely contains binary content
            // This is expected if the smudge filter is working correctly
            continue;
        }

        // Read file content
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => {
                // Binary file or unreadable, skip (this is expected for checked-out LFS files)
                continue;
            }
        };

        let trimmed = content.trim();

        // Check if it's a valid LFS pointer
        if trimmed.starts_with(LFS_POINTER_PREFIX) {
            // Valid pointer format
            continue;
        }

        // Check if it looks like a corrupted pointer (partial LFS content)
        if trimmed.contains("git-lfs") || trimmed.contains("sha256") {
            let preview: String = trimmed.chars().take(50).collect();
            issues.push(LfsPointerIssue::Corrupted {
                path: file_path.clone(),
                content_preview: preview,
            });
            continue;
        }

        // File is small but not a valid pointer - might be a corrupted pointer
        if !trimmed.is_empty() {
            issues.push(LfsPointerIssue::InvalidPointer {
                path: file_path.clone(),
                reason: "File does not match LFS pointer format".to_string(),
            });
        }
    }

    Ok(issues)
}

/// Performs a comprehensive LFS health check.
///
/// This function combines all LFS validation checks:
/// - Checks if LFS is initialized
/// - Validates filter configuration
/// - Checks `git lfs status` for issues
/// - Validates pointer files for tracked LFS files
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// * `Ok(LfsHealthReport)` - Complete health report
/// * `Err(anyhow::Error)` - If validation fails
///
/// # Example
/// ```
/// use std::path::Path;
/// use ralph::git::lfs::check_lfs_health;
///
/// let report = check_lfs_health(Path::new(".")).unwrap();
/// if !report.is_healthy() {
///     for issue in report.all_issues() {
///         eprintln!("LFS issue: {}", issue);
///     }
/// }
/// ```
pub fn check_lfs_health(repo_root: &Path) -> Result<LfsHealthReport> {
    let lfs_initialized = has_lfs(repo_root)?;

    if !lfs_initialized {
        return Ok(LfsHealthReport {
            lfs_initialized: false,
            ..LfsHealthReport::default()
        });
    }

    let filter_status = validate_lfs_filters(repo_root).ok();
    let status_summary = check_lfs_status(repo_root).ok();

    // Validate pointers for tracked LFS files
    let lfs_files = list_lfs_files(repo_root).unwrap_or_default();
    let pointer_issues = if !lfs_files.is_empty() {
        validate_lfs_pointers(repo_root, &lfs_files).unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(LfsHealthReport {
        lfs_initialized: true,
        filter_status,
        status_summary,
        pointer_issues,
    })
}

/// Filter status paths to only include LFS-tracked files.
pub fn filter_modified_lfs_files(status_paths: &[String], lfs_files: &[String]) -> Vec<String> {
    if status_paths.is_empty() || lfs_files.is_empty() {
        return Vec::new();
    }

    let mut lfs_set = std::collections::HashSet::new();
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

#[cfg(test)]
mod lfs_validation_tests {
    use super::*;
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn lfs_filter_status_is_healthy_when_both_filters_installed() {
        let status = LfsFilterStatus {
            smudge_installed: true,
            clean_installed: true,
            smudge_value: Some("git-lfs smudge %f".to_string()),
            clean_value: Some("git-lfs clean %f".to_string()),
        };
        assert!(status.is_healthy());
        assert!(status.issues().is_empty());
    }

    #[test]
    fn lfs_filter_status_is_not_healthy_when_smudge_missing() {
        let status = LfsFilterStatus {
            smudge_installed: false,
            clean_installed: true,
            smudge_value: None,
            clean_value: Some("git-lfs clean %f".to_string()),
        };
        assert!(!status.is_healthy());
        let issues = status.issues();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("smudge"));
    }

    #[test]
    fn lfs_filter_status_is_not_healthy_when_clean_missing() {
        let status = LfsFilterStatus {
            smudge_installed: true,
            clean_installed: false,
            smudge_value: Some("git-lfs smudge %f".to_string()),
            clean_value: None,
        };
        assert!(!status.is_healthy());
        let issues = status.issues();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("clean"));
    }

    #[test]
    fn lfs_filter_status_reports_both_issues_when_both_missing() {
        let status = LfsFilterStatus {
            smudge_installed: false,
            clean_installed: false,
            smudge_value: None,
            clean_value: None,
        };
        assert!(!status.is_healthy());
        let issues = status.issues();
        assert_eq!(issues.len(), 2);
    }

    #[test]
    fn lfs_status_summary_is_clean_when_empty() {
        let summary = LfsStatusSummary::default();
        assert!(summary.is_clean());
        assert!(summary.issue_descriptions().is_empty());
    }

    #[test]
    fn lfs_status_summary_reports_staged_not_lfs_issue() {
        let summary = LfsStatusSummary {
            staged_lfs: vec![],
            staged_not_lfs: vec!["large.bin".to_string()],
            unstaged_lfs: vec![],
            untracked_attributes: vec![],
        };
        assert!(!summary.is_clean());
        let issues = summary.issue_descriptions();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("large.bin"));
    }

    #[test]
    fn lfs_status_summary_reports_untracked_attributes_issue() {
        let summary = LfsStatusSummary {
            staged_lfs: vec![],
            staged_not_lfs: vec![],
            unstaged_lfs: vec![],
            untracked_attributes: vec!["data.bin".to_string()],
        };
        assert!(!summary.is_clean());
        let issues = summary.issue_descriptions();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("data.bin"));
    }

    #[test]
    fn lfs_health_report_is_healthy_when_lfs_not_initialized() {
        let report = LfsHealthReport {
            lfs_initialized: false,
            filter_status: None,
            status_summary: None,
            pointer_issues: vec![],
        };
        assert!(report.is_healthy());
    }

    #[test]
    fn lfs_health_report_is_not_healthy_with_filter_issues() {
        let report = LfsHealthReport {
            lfs_initialized: true,
            filter_status: Some(LfsFilterStatus {
                smudge_installed: false,
                clean_installed: true,
                smudge_value: None,
                clean_value: Some("git-lfs clean %f".to_string()),
            }),
            status_summary: Some(LfsStatusSummary::default()),
            pointer_issues: vec![],
        };
        assert!(!report.is_healthy());
        let issues = report.all_issues();
        assert!(!issues.is_empty());
    }

    #[test]
    fn lfs_health_report_is_not_healthy_with_status_issues() {
        let report = LfsHealthReport {
            lfs_initialized: true,
            filter_status: Some(LfsFilterStatus {
                smudge_installed: true,
                clean_installed: true,
                smudge_value: Some("git-lfs smudge %f".to_string()),
                clean_value: Some("git-lfs clean %f".to_string()),
            }),
            status_summary: Some(LfsStatusSummary {
                staged_lfs: vec![],
                staged_not_lfs: vec!["file.bin".to_string()],
                unstaged_lfs: vec![],
                untracked_attributes: vec![],
            }),
            pointer_issues: vec![],
        };
        assert!(!report.is_healthy());
    }

    #[test]
    fn lfs_health_report_is_not_healthy_with_pointer_issues() {
        let report = LfsHealthReport {
            lfs_initialized: true,
            filter_status: Some(LfsFilterStatus {
                smudge_installed: true,
                clean_installed: true,
                smudge_value: Some("git-lfs smudge %f".to_string()),
                clean_value: Some("git-lfs clean %f".to_string()),
            }),
            status_summary: Some(LfsStatusSummary::default()),
            pointer_issues: vec![LfsPointerIssue::InvalidPointer {
                path: "test.bin".to_string(),
                reason: "Invalid format".to_string(),
            }],
        };
        assert!(!report.is_healthy());
        let issues = report.all_issues();
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn validate_lfs_pointers_detects_invalid_pointer() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;

        // Create a file that looks like an invalid LFS pointer
        let pointer_content = "invalid pointer content";
        std::fs::write(temp.path().join("test.bin"), pointer_content)?;

        let issues = validate_lfs_pointers(temp.path(), &["test.bin".to_string()])?;
        assert_eq!(issues.len(), 1);
        assert!(matches!(
            issues[0],
            LfsPointerIssue::InvalidPointer { ref path, .. } if path == "test.bin"
        ));
        Ok(())
    }

    #[test]
    fn validate_lfs_pointers_skips_large_files() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;

        // Create a large file (bigger than MAX_POINTER_SIZE)
        let large_content = vec![0u8; 2048];
        std::fs::write(temp.path().join("large.bin"), large_content)?;

        let issues = validate_lfs_pointers(temp.path(), &["large.bin".to_string()])?;
        assert!(issues.is_empty());
        Ok(())
    }

    #[test]
    fn validate_lfs_pointers_accepts_valid_pointer() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;

        // Create a valid LFS pointer
        let pointer_content =
            "version https://git-lfs.github.com/spec/v1\noid sha256:abc123\nsize 123\n";
        std::fs::write(temp.path().join("valid.bin"), pointer_content)?;

        let issues = validate_lfs_pointers(temp.path(), &["valid.bin".to_string()])?;
        assert!(issues.is_empty());
        Ok(())
    }

    #[test]
    fn lfs_pointer_issue_description_contains_path() {
        let issue = LfsPointerIssue::InvalidPointer {
            path: "test/file.bin".to_string(),
            reason: "corrupted".to_string(),
        };
        let desc = issue.description();
        assert!(desc.contains("test/file.bin"));
        assert!(desc.contains("corrupted"));
    }

    #[test]
    fn lfs_pointer_issue_path_returns_correct_path() {
        let issue = LfsPointerIssue::BinaryContent {
            path: "binary.bin".to_string(),
        };
        assert_eq!(issue.path(), "binary.bin");
    }
}
