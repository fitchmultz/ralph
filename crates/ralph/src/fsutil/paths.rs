//! Purpose: Path normalization helpers for filesystem-facing callers.
//!
//! Responsibilities:
//! - Expand supported Unix-style tilde paths into home-directory paths.
//! - Preserve untouched paths when expansion is unsupported or unavailable.
//!
//! Scope:
//! - Leading-tilde path expansion only; temp files, atomic writes, and safeguard dumps live elsewhere.
//!
//! Usage:
//! - Used by config, git, prompt, and other callers that accept user-supplied filesystem paths.
//!
//! Invariants/Assumptions:
//! - Only leading `~` and `~/...` forms are expanded.
//! - If `$HOME` is unset or blank, the original path is returned unchanged.
//! - Username-based tilde expansion and nested tildes are intentionally unsupported.

use std::path::{Path, PathBuf};

/// Expands a leading `~` to the user's home directory (`$HOME`) for Unix-style paths.
///
/// Supported:
/// - `~` → `$HOME`
/// - `~/...` → `$HOME/...`
///
/// Not handled (intentionally):
/// - `~user/...` (username-based expansion)
/// - `.../~/...` (nested tilde)
/// - Windows `%USERPROFILE%` expansion (callers should supply absolute paths)
///
/// If `$HOME` is unset or empty, the input path is returned unchanged.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();

    let home = std::env::var("HOME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let Some(home) = home else {
        log::debug!(
            "HOME environment variable not set; skipping tilde expansion for path: {}",
            raw
        );
        return path.to_path_buf();
    };

    if raw == "~" {
        return PathBuf::from(home);
    }

    if let Some(rest) = raw.strip_prefix("~/") {
        // Avoid `PathBuf::join` treating `rest` as absolute if user wrote "~//foo".
        let rest = rest.trim_start_matches(&['/', '\\'][..]);
        return PathBuf::from(home).join(rest);
    }

    path.to_path_buf()
}
