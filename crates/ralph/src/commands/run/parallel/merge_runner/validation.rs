//! PR head validation for merge runner.
//!
//! Responsibilities:
//! - Validate PR head branch names match expected naming conventions.
//!
//! Not handled here:
//! - Merge execution (see `mod.rs`).
//! - Conflict resolution (see `conflict.rs`).

/// Validates that the PR head matches the expected branch naming convention.
///
/// Expected format: `{branch_prefix}{task_id}`
/// Returns `Ok(())` if valid, or an error message if invalid.
pub(crate) fn validate_pr_head(
    branch_prefix: &str,
    task_id: &str,
    head: &str,
) -> Result<(), String> {
    let expected = format!("{}{}", branch_prefix, task_id);
    let trimmed_head = head.trim();

    if trimmed_head != expected {
        return Err(format!(
            "head mismatch: expected '{}', got '{}'",
            expected, trimmed_head
        ));
    }

    // Additional safety: reject path separators and parent directory references
    if task_id.contains('/') || task_id.contains("..") {
        return Err(format!(
            "invalid task_id '{}': contains path separators",
            task_id
        ));
    }

    Ok(())
}
