//! Queue file size checking and warning generation.
//!
//! Responsibilities:
//! - Check if queue file exceeds size or task count thresholds.
//! - Generate user-friendly warning messages with remediation suggestions.
//!
//! Not handled here:
//! - Configuration loading (passed in by caller).
//! - Actual queue operations (archive/prune) - only suggestions.

use crate::constants::limits::{
    DEFAULT_SIZE_WARNING_THRESHOLD_KB, DEFAULT_TASK_COUNT_WARNING_THRESHOLD,
};
use std::path::Path;

use anyhow::{Context, Result};

/// Information about the queue file size and task count.
#[derive(Debug, Clone, Copy)]
pub struct QueueSizeInfo {
    /// File size in kilobytes.
    pub file_size_kb: u64,
    /// Number of tasks in the queue.
    pub task_count: usize,
}

/// Result of checking queue size against thresholds.
#[derive(Debug, Clone, Copy)]
pub struct SizeCheckResult {
    /// Whether the file size exceeds the threshold.
    pub exceeds_size_threshold: bool,
    /// Whether the task count exceeds the threshold.
    pub exceeds_count_threshold: bool,
    /// The size information that was checked.
    pub info: QueueSizeInfo,
    /// The size threshold that was used for the check (in KB).
    pub size_threshold_kb: u32,
    /// The task count threshold that was used for the check.
    pub count_threshold: u32,
}

/// Check if queue exceeds configured thresholds.
///
/// # Arguments
/// * `queue_path` - Path to the queue file.
/// * `task_count` - Number of tasks in the queue.
/// * `size_threshold_kb` - Threshold for file size warning in KB.
/// * `count_threshold` - Threshold for task count warning.
///
/// # Returns
/// A `SizeCheckResult` indicating which thresholds (if any) were exceeded.
pub fn check_queue_size(
    queue_path: &Path,
    task_count: usize,
    size_threshold_kb: u32,
    count_threshold: u32,
) -> Result<SizeCheckResult> {
    let metadata = std::fs::metadata(queue_path)
        .with_context(|| format!("read metadata for {}", queue_path.display()))?;

    let file_size_bytes = metadata.len();
    let file_size_kb = file_size_bytes / 1024;

    let exceeds_size_threshold = file_size_kb > u64::from(size_threshold_kb);
    let exceeds_count_threshold = task_count > count_threshold as usize;

    Ok(SizeCheckResult {
        exceeds_size_threshold,
        exceeds_count_threshold,
        info: QueueSizeInfo {
            file_size_kb,
            task_count,
        },
        size_threshold_kb,
        count_threshold,
    })
}

/// Print warning if thresholds exceeded (respects quiet flag).
///
/// # Arguments
/// * `result` - The result from `check_queue_size`.
/// * `quiet` - If true, suppresses the warning output.
pub fn print_size_warning_if_needed(result: &SizeCheckResult, quiet: bool) {
    if quiet {
        return;
    }

    if !result.exceeds_size_threshold && !result.exceeds_count_threshold {
        return;
    }

    eprintln!();
    eprintln!("⚠️  Queue size warning:");

    if result.exceeds_size_threshold {
        eprintln!(
            "   Queue file size is {}KB (threshold: {}KB)",
            result.info.file_size_kb, result.size_threshold_kb
        );
    }

    if result.exceeds_count_threshold {
        eprintln!(
            "   Queue has {} tasks (threshold: {})",
            result.info.task_count, result.count_threshold
        );
    }

    eprintln!();
    eprintln!("   Consider running maintenance commands:");
    eprintln!("     ralph queue archive    # Move completed tasks to done.json");
    eprintln!("     ralph queue prune      # Remove old tasks from done.json");
    eprintln!();
}

/// Get the configured size threshold, or the default.
pub fn size_threshold_or_default(threshold: Option<u32>) -> u32 {
    threshold.unwrap_or(DEFAULT_SIZE_WARNING_THRESHOLD_KB)
}

/// Get the configured task count threshold, or the default.
pub fn count_threshold_or_default(threshold: Option<u32>) -> u32 {
    threshold.unwrap_or(DEFAULT_TASK_COUNT_WARNING_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn check_queue_size_detects_size_threshold() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Create a file that's 1KB
        let content = "x".repeat(1024);
        std::fs::write(&queue_path, content)?;

        // Threshold of 500 bytes (0.5KB) - should trigger
        let result = check_queue_size(&queue_path, 10, 0, 100)?;
        assert!(result.exceeds_size_threshold);
        assert!(!result.exceeds_count_threshold);
        assert_eq!(result.info.file_size_kb, 1);

        Ok(())
    }

    #[test]
    fn check_queue_size_detects_count_threshold() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Small file
        std::fs::write(&queue_path, r#"{"tasks": []}"#)?;

        // Threshold of 10 tasks - should trigger with 15 tasks
        let result = check_queue_size(&queue_path, 15, 1000, 10)?;
        assert!(!result.exceeds_size_threshold);
        assert!(result.exceeds_count_threshold);
        assert_eq!(result.info.task_count, 15);

        Ok(())
    }

    #[test]
    fn check_queue_size_no_threshold_exceeded() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Small file
        std::fs::write(&queue_path, r#"{"tasks": []}"#)?;

        // High thresholds - should not trigger
        let result = check_queue_size(&queue_path, 10, 10000, 5000)?;
        assert!(!result.exceeds_size_threshold);
        assert!(!result.exceeds_count_threshold);

        Ok(())
    }

    #[test]
    fn check_queue_size_detects_both_thresholds() -> Result<()> {
        let temp = TempDir::new()?;
        let queue_path = temp.path().join("queue.json");

        // Create a file larger than 1KB
        let content = "x".repeat(2048);
        std::fs::write(&queue_path, content)?;

        // Both thresholds should trigger
        let result = check_queue_size(&queue_path, 1000, 1, 500)?;
        assert!(result.exceeds_size_threshold);
        assert!(result.exceeds_count_threshold);

        Ok(())
    }

    #[test]
    fn threshold_helpers_return_defaults() {
        assert_eq!(
            size_threshold_or_default(None),
            DEFAULT_SIZE_WARNING_THRESHOLD_KB
        );
        assert_eq!(
            count_threshold_or_default(None),
            DEFAULT_TASK_COUNT_WARNING_THRESHOLD
        );
    }

    #[test]
    fn threshold_helpers_return_configured() {
        assert_eq!(size_threshold_or_default(Some(1000)), 1000);
        assert_eq!(count_threshold_or_default(Some(1000)), 1000);
    }
}
