//! Purpose: Instruction-file loading and validation helpers for prompt composition.
//!
//! Responsibilities:
//! - Resolve configured instruction-file paths against the repo root.
//! - Read, size-check, and validate instruction-file contents.
//! - Wrap prompts with explicit instruction-file preambles and warnings.
//!
//! Scope:
//! - Explicitly configured instruction files only.
//! - Does not expand template variables or render general prompts.
//!
//! Usage:
//! - Used by prompt wrappers and config validation.
//! - Re-exported from `crate::prompts_internal` for crate-local callers.
//!
//! Invariants/Assumptions:
//! - Instruction files are UTF-8, non-empty, and bounded by `MAX_INSTRUCTION_BYTES`.
//! - Repo-relative paths resolve against the current repo root.
//! - Unconfigured instruction files are never auto-injected.

use crate::constants::buffers::MAX_INSTRUCTION_BYTES;
use crate::contracts::Config;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn wrap_with_instruction_files(
    repo_root: &Path,
    prompt: &str,
    config: &Config,
) -> Result<String> {
    let mut sources: Vec<(String, String)> = Vec::new();

    // Instruction files from configuration (user-specified, not auto-injected).
    if let Some(paths) = config.agent.instruction_files.as_ref() {
        for raw in paths {
            let resolved = resolve_instruction_path(repo_root, raw);
            let content = read_instruction_file(&resolved, MAX_INSTRUCTION_BYTES)
                .with_context(|| format!("read instruction file at {}", resolved.display()))?;
            sources.push((resolved.display().to_string(), content));
        }
    }

    if sources.is_empty() {
        return Ok(prompt.to_string());
    }

    let mut preamble = String::new();
    preamble.push_str(
        r#"## AGENTS / GLOBAL INSTRUCTIONS (AUTHORITATIVE)
The following configured instruction files are authoritative for this run. Follow hard invariants exactly, and apply advisory guidance with outcome-first judgment.

"#,
    );

    for (idx, (label, content)) in sources.into_iter().enumerate() {
        if idx > 0 {
            preamble.push_str("\n---\n\n");
        }
        preamble.push_str(&format!("### Source: {label}\n\n"));
        preamble.push_str(content.trim());
        preamble.push('\n');
    }

    Ok(format!("{}\n\n---\n\n{}", preamble.trim(), prompt))
}

pub(crate) fn instruction_file_warnings(repo_root: &Path, config: &Config) -> Vec<String> {
    let mut warnings = Vec::new();

    // Only check configured instruction files (no auto-injection).
    if let Some(paths) = config.agent.instruction_files.as_ref() {
        for raw in paths {
            let resolved = resolve_instruction_path(repo_root, raw);
            if let Err(err) = read_instruction_file(&resolved, MAX_INSTRUCTION_BYTES) {
                warnings.push(format!(
                    "instruction_files entry '{}' (resolved: {}) is invalid: {}",
                    raw.display(),
                    resolved.display(),
                    err
                ));
            }
        }
    }

    warnings
}

/// Validates all instruction files in config and returns first error encountered.
/// Used for early config validation (fails fast) during config resolution.
pub(crate) fn validate_instruction_file_paths(repo_root: &Path, config: &Config) -> Result<()> {
    if let Some(paths) = config.agent.instruction_files.as_ref() {
        for raw in paths {
            let resolved = resolve_instruction_path(repo_root, raw);
            // read_instruction_file returns Err if file doesn't exist, isn't UTF-8, or is empty
            if let Err(err) = read_instruction_file(&resolved, MAX_INSTRUCTION_BYTES) {
                bail!(
                    "Invalid instruction_files entry '{}': {}. \
                     Ensure the file exists, is readable, and contains valid UTF-8 content.",
                    raw.display(),
                    err
                );
            }
        }
    }
    Ok(())
}

fn resolve_instruction_path(repo_root: &Path, raw: &Path) -> PathBuf {
    let expanded = crate::fsutil::expand_tilde(raw);

    if expanded.is_absolute() {
        expanded
    } else {
        repo_root.join(expanded)
    }
}

fn read_instruction_file(path: &Path, max_bytes: usize) -> Result<String> {
    let data = fs::read(path).with_context(|| format!("read bytes from {}", path.display()))?;
    if data.len() > max_bytes {
        bail!(
            "instruction file {} is too large ({} bytes > {} bytes max)",
            path.display(),
            data.len(),
            max_bytes
        );
    }
    let text = String::from_utf8(data).map_err(|e| {
        anyhow::anyhow!(
            "instruction file {} is not valid UTF-8: {}",
            path.display(),
            e
        )
    })?;
    if text.trim().is_empty() {
        bail!("instruction file {} is empty", path.display());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{instruction_file_warnings, resolve_instruction_path, wrap_with_instruction_files};
    use crate::contracts::Config;
    use serial_test::serial;
    use std::env;
    use std::path::Path;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Global lock for environment variable tests
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn wrap_with_instruction_files_is_noop_when_none_configured() {
        let dir = TempDir::new().expect("tempdir");
        // Even if AGENTS.md exists, it should NOT be injected without explicit configuration
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        let cfg = Config::default();
        let out = wrap_with_instruction_files(dir.path(), "hello", &cfg).expect("wrap");
        assert_eq!(out, "hello");
    }

    #[test]
    fn wrap_with_instruction_files_includes_agents_md_when_explicitly_configured() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        let mut cfg = Config::default();
        // Explicitly configure AGENTS.md for injection
        cfg.agent.instruction_files = Some(vec![Path::new("AGENTS.md").to_path_buf()]);

        let out = wrap_with_instruction_files(dir.path(), "hello", &cfg).expect("wrap");
        assert!(out.contains("AGENTS / GLOBAL INSTRUCTIONS"));
        assert!(out.contains("Repo instructions"));
        assert!(out.ends_with("\n\n---\n\nhello"));
    }

    #[test]
    fn wrap_with_instruction_files_does_not_include_repo_agents_md_when_not_configured() {
        let dir = TempDir::new().expect("tempdir");
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        // Config with no instruction_files - AGENTS.md should NOT be auto-injected
        let cfg = Config::default();

        let out = wrap_with_instruction_files(dir.path(), "hello", &cfg).expect("wrap");
        // Should be exactly the original prompt with no preamble
        assert_eq!(out, "hello");
        assert!(!out.contains("AGENTS / GLOBAL INSTRUCTIONS"));
        assert!(!out.contains("Repo instructions"));
    }

    #[test]
    fn wrap_with_instruction_files_errors_on_missing_configured_file() {
        let dir = TempDir::new().expect("tempdir");
        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![Path::new("missing.md").to_path_buf()]);

        let err = wrap_with_instruction_files(dir.path(), "hello", &cfg).unwrap_err();
        assert!(err.to_string().contains("missing.md"));
    }

    #[test]
    fn instruction_file_warnings_reports_missing_configured_file() {
        let dir = TempDir::new().expect("tempdir");
        let mut cfg = Config::default();
        cfg.agent.instruction_files = Some(vec![Path::new("missing.md").to_path_buf()]);

        let warnings = instruction_file_warnings(dir.path(), &cfg);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("instruction_files"));
        assert!(warnings[0].contains("missing.md"));
    }

    #[test]
    fn instruction_file_warnings_does_not_warn_about_unconfigured_repo_agents_md() {
        let dir = TempDir::new().expect("tempdir");
        // Create AGENTS.md but do NOT configure it
        std::fs::write(dir.path().join("AGENTS.md"), "Repo instructions").expect("write");
        let cfg = Config::default();

        let warnings = instruction_file_warnings(dir.path(), &cfg);
        // Should have no warnings since AGENTS.md is not configured
        assert!(
            warnings.is_empty(),
            "Expected no warnings for unconfigured AGENTS.md"
        );
    }

    #[test]
    #[serial]
    fn resolve_instruction_path_expands_tilde_to_home() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let original_home = env::var("HOME").ok();

        unsafe { env::set_var("HOME", "/custom/home") };

        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("~/instructions.md"));
        assert_eq!(resolved, Path::new("/custom/home/instructions.md"));

        // Restore HOME
        match original_home {
            Some(v) => unsafe { env::set_var("HOME", v) },
            None => unsafe { env::remove_var("HOME") },
        }
    }

    #[test]
    #[serial]
    fn resolve_instruction_path_expands_tilde_alone_to_home() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let original_home = env::var("HOME").ok();

        unsafe { env::set_var("HOME", "/custom/home") };

        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("~"));
        assert_eq!(resolved, Path::new("/custom/home"));

        // Restore HOME
        match original_home {
            Some(v) => unsafe { env::set_var("HOME", v) },
            None => unsafe { env::remove_var("HOME") },
        }
    }

    #[test]
    #[serial]
    fn resolve_instruction_path_relative_when_home_unset() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let original_home = env::var("HOME").ok();

        // Remove HOME - tilde should not expand
        unsafe { env::remove_var("HOME") };

        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("~/instructions.md"));
        // When HOME is unset, ~/instructions.md is treated as relative to repo_root
        assert_eq!(resolved, Path::new("/repo/root/~/instructions.md"));

        // Restore HOME
        if let Some(v) = original_home {
            unsafe { env::set_var("HOME", v) }
        }
    }

    #[test]
    fn resolve_instruction_path_absolute_unchanged() {
        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("/absolute/path/file.md"));
        assert_eq!(resolved, Path::new("/absolute/path/file.md"));
    }

    #[test]
    fn resolve_instruction_path_relative_unchanged() {
        let repo_root = Path::new("/repo/root");
        let resolved = resolve_instruction_path(repo_root, Path::new("relative/path/file.md"));
        assert_eq!(resolved, Path::new("/repo/root/relative/path/file.md"));
    }
}
