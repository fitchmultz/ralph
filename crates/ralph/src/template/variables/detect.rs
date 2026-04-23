//! Purpose: Derive template substitution context from targets, the filesystem,
//! and git state.
//!
//! Responsibilities:
//! - Convert optional target paths into file/module context.
//! - Detect git branch information when requested.
//! - Surface git-detection failures as template warnings.
//!
//! Scope:
//! - Context derivation and git probing only; no validation or substitution.
//!
//! Usage:
//! - Called after template validation determines whether branch context is
//!   required.
//!
//! Invariants/Assumptions:
//! - `detect_context` keeps legacy behavior by always attempting branch lookup.
//! - Git probing preserves the `.git/HEAD` fast path and managed-subprocess
//!   fallback semantics.
//! - Module derivation behavior remains unchanged.

use std::path::Path;

use anyhow::{Context, Result};

use crate::git::error::git_probe_stdout;

use super::context::{TemplateContext, TemplateWarning};

/// Detect context from target path and git repository.
///
/// Returns the context and any warnings (e.g., git branch detection failures).
/// Only attempts git branch detection if the template uses {{branch}}.
pub fn detect_context_with_warnings(
    target: Option<&str>,
    repo_root: &Path,
    needs_branch: bool,
) -> (TemplateContext, Vec<TemplateWarning>) {
    let mut warnings = Vec::new();
    let target_opt = target.map(|s| s.to_string());

    let file = target_opt.as_ref().map(|t| {
        Path::new(t)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| t.clone())
    });

    let module = target_opt.as_ref().map(|t| derive_module_name(t));

    let branch = if needs_branch {
        match detect_git_branch(repo_root) {
            Ok(branch_opt) => branch_opt,
            Err(e) => {
                warnings.push(TemplateWarning::GitBranchDetectionFailed {
                    error: e.to_string(),
                });
                None
            }
        }
    } else {
        None
    };

    let context = TemplateContext {
        target: target_opt,
        file,
        module,
        branch,
    };

    (context, warnings)
}

/// Detect context from target path and git repository (legacy, ignores warnings).
pub fn detect_context(target: Option<&str>, repo_root: &Path) -> TemplateContext {
    let (context, _) = detect_context_with_warnings(target, repo_root, true);
    context
}

/// Derive a module name from a file path.
///
/// Examples:
/// - "src/cli/task.rs" -> "cli::task"
/// - "crates/ralph/src/main.rs" -> "ralph::main"
/// - "lib/utils.js" -> "utils"
pub(super) fn derive_module_name(path: &str) -> String {
    let path_obj = Path::new(path);

    // Get the file stem (filename without extension)
    let file_stem = path_obj
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let mut components: Vec<String> = Vec::new();

    // Walk through parent directories looking for meaningful names
    for component in path_obj.components() {
        let comp_str = component.as_os_str().to_string_lossy().to_string();

        // Skip common non-module directories
        if comp_str == "src"
            || comp_str == "lib"
            || comp_str == "bin"
            || comp_str == "tests"
            || comp_str == "examples"
            || comp_str == "crates"
        {
            continue;
        }

        // Skip the filename itself (we use file_stem separately)
        if comp_str
            == path_obj
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        {
            continue;
        }

        components.push(comp_str);
    }

    // If we found meaningful components, combine with file stem
    if !components.is_empty() {
        components.push(file_stem);
        components.join("::")
    } else {
        file_stem
    }
}

/// Detect the current git branch name.
fn detect_git_branch(repo_root: &Path) -> Result<Option<String>> {
    // Try to read from git HEAD
    let head_path = repo_root.join(".git/HEAD");

    if !head_path.exists() {
        // Worktrees and submodules may expose `.git` as a file, so fall back to git itself.
        let branch = git_probe_stdout(repo_root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .context("failed to detect template git branch")?;

        if branch != "HEAD" {
            return Ok(Some(branch));
        }
        return Ok(None);
    }

    let head_content = std::fs::read_to_string(&head_path)
        .with_context(|| format!("failed to read {:?}", head_path))?;
    let head_ref = head_content.trim();

    // HEAD content is like: "ref: refs/heads/main"
    if head_ref.starts_with("ref: refs/heads/") {
        let branch = head_ref
            .strip_prefix("ref: refs/heads/")
            .unwrap_or(head_ref)
            .to_string();
        Ok(Some(branch))
    } else if head_ref.len() == 40 && head_ref.chars().all(|c| c.is_ascii_hexdigit()) {
        // Detached HEAD state (40-character hex commit SHA)
        Ok(None)
    } else if head_ref.is_empty() {
        Err(anyhow::anyhow!("HEAD file is empty"))
    } else {
        // Invalid HEAD content
        Err(anyhow::anyhow!("invalid HEAD content: {}", head_ref))
    }
}
