//! Gitignore management for Ralph initialization.
//!
//! Responsibilities:
//! - Ensure `.ralph/workspaces/` is in `.gitignore` to prevent dirty repo issues.
//! - Provide idempotent updates to `.gitignore`.
//!
//! Not handled here:
//! - Reading or parsing existing `.gitignore` patterns (only simple line-based checks).
//! - Global gitignore configuration (only repo-local `.gitignore`).
//!
//! Invariants/assumptions:
//! - Updates are additive only (never removes entries).
//! - Safe to run multiple times (idempotent).

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Ensures Ralph-specific entries exist in `.gitignore`.
///
/// Currently ensures:
/// - `.ralph/workspaces/` is ignored (prevents dirty repo when using repo-local workspaces)
///
/// This function is idempotent - calling it multiple times is safe.
pub fn ensure_ralph_gitignore_entries(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");

    // Read existing content or start fresh
    let existing_content = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)
            .with_context(|| format!("read {}", gitignore_path.display()))?
    } else {
        String::new()
    };

    // Check if entry already exists (handle various formats)
    let needs_workspaces_entry = !existing_content.lines().any(is_workspaces_ignore_entry);

    if !needs_workspaces_entry {
        log::debug!(".ralph/workspaces/ already in .gitignore");
        return Ok(());
    }

    // Append the entry
    let mut new_content = existing_content;

    // Add newline if file doesn't end with one (and isn't empty)
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }

    // Add a comment explaining the entry
    if !new_content.is_empty() {
        new_content.push('\n');
    }
    new_content.push_str("# Ralph parallel mode workspace directories\n");
    new_content.push_str(".ralph/workspaces/\n");

    fs::write(&gitignore_path, new_content)
        .with_context(|| format!("write {}", gitignore_path.display()))?;

    log::info!("Added '.ralph/workspaces/' to .gitignore");
    Ok(())
}

/// Check if a line is a workspaces ignore entry.
///
/// Matches:
/// - `.ralph/workspaces/`
/// - `.ralph/workspaces`
fn is_workspaces_ignore_entry(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == ".ralph/workspaces/" || trimmed == ".ralph/workspaces"
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ensure_ralph_gitignore_entries_creates_new_file() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();

        ensure_ralph_gitignore_entries(repo_root)?;

        let gitignore_path = repo_root.join(".gitignore");
        assert!(gitignore_path.exists());
        let content = fs::read_to_string(&gitignore_path)?;
        assert!(content.contains(".ralph/workspaces/"));
        assert!(content.contains("# Ralph parallel mode"));
        Ok(())
    }

    #[test]
    fn ensure_ralph_gitignore_entries_appends_to_existing() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let gitignore_path = repo_root.join(".gitignore");
        fs::write(&gitignore_path, ".env\ntarget/\n")?;

        ensure_ralph_gitignore_entries(repo_root)?;

        let content = fs::read_to_string(&gitignore_path)?;
        assert!(content.contains(".env"));
        assert!(content.contains("target/"));
        assert!(content.contains(".ralph/workspaces/"));
        Ok(())
    }

    #[test]
    fn ensure_ralph_gitignore_entries_is_idempotent() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();

        // Run twice
        ensure_ralph_gitignore_entries(repo_root)?;
        ensure_ralph_gitignore_entries(repo_root)?;

        let gitignore_path = repo_root.join(".gitignore");
        let content = fs::read_to_string(&gitignore_path)?;

        // Should only have one entry
        let count = content.matches(".ralph/workspaces/").count();
        assert_eq!(count, 1, "Should only have one .ralph/workspaces/ entry");
        Ok(())
    }

    #[test]
    fn ensure_ralph_gitignore_entries_detects_existing_entry_with_trailing_slash() -> Result<()> {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let gitignore_path = repo_root.join(".gitignore");
        fs::write(&gitignore_path, ".ralph/workspaces/\n")?;

        ensure_ralph_gitignore_entries(repo_root)?;

        let content = fs::read_to_string(&gitignore_path)?;
        let count = content.matches(".ralph/workspaces/").count();
        assert_eq!(count, 1, "Should not add duplicate");
        Ok(())
    }

    #[test]
    fn ensure_ralph_gitignore_entries_detects_existing_entry_without_trailing_slash() -> Result<()>
    {
        let temp = TempDir::new()?;
        let repo_root = temp.path();
        let gitignore_path = repo_root.join(".gitignore");
        fs::write(&gitignore_path, ".ralph/workspaces\n")?;

        ensure_ralph_gitignore_entries(repo_root)?;

        let content = fs::read_to_string(&gitignore_path)?;
        // Should not add the trailing-slash version if non-trailing exists
        let count = content
            .lines()
            .filter(|l| l.contains(".ralph/workspaces"))
            .count();
        assert_eq!(count, 1, "Should not add duplicate");
        Ok(())
    }
}
