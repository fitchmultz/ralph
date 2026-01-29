//! Filesystem helpers for temp directories, atomic writes, and safeguard dumps.
//!
//! Responsibilities:
//! - Create and clean Ralph temp directories.
//! - Write files atomically and sync parent directories best-effort.
//! - Persist safeguard dumps for troubleshooting output.
//! - Redact sensitive data in safeguard dumps by default (secrets, API keys, tokens).
//!
//! Not handled here:
//! - Directory locks or lock ownership metadata (see `crate::lock`).
//! - Cross-device file moves or distributed filesystem semantics.
//! - Retry/backoff behavior beyond the current best-effort operations.
//! - Redaction logic itself (see `crate::redaction`).
//!
//! Invariants/assumptions:
//! - Callers provide valid paths; `write_atomic` requires a parent directory.
//! - Temp cleanup is best-effort and may skip entries on IO errors.
//! - `safeguard_text_dump` requires explicit opt-in (env var or debug mode) to write raw content.
//! - `safeguard_text_dump_redacted` is the default and safe choice for error dumps.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const RALPH_TEMP_DIR_NAME: &str = "ralph";
const LEGACY_PROMPT_PREFIX: &str = "ralph_prompt_";
pub const RALPH_TEMP_PREFIX: &str = "ralph_";

pub fn ralph_temp_root() -> PathBuf {
    std::env::temp_dir().join(RALPH_TEMP_DIR_NAME)
}

pub fn cleanup_stale_temp_entries(
    base: &Path,
    prefixes: &[&str],
    retention: Duration,
) -> Result<usize> {
    if !base.exists() {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut removed = 0usize;

    for entry in fs::read_dir(base).with_context(|| format!("read temp dir {}", base.display()))? {
        let entry = entry.with_context(|| format!("read temp dir entry in {}", base.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                log::warn!(
                    "unable to read temp metadata for {}: {}",
                    path.display(),
                    err
                );
                continue;
            }
        };

        let modified = match metadata.modified() {
            Ok(time) => time,
            Err(err) => {
                log::warn!(
                    "unable to read temp modified time for {}: {}",
                    path.display(),
                    err
                );
                continue;
            }
        };

        let age = match now.duration_since(modified) {
            Ok(age) => age,
            Err(_) => continue,
        };

        if age < retention {
            continue;
        }

        if metadata.is_dir() {
            if fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            } else {
                log::warn!("failed to remove temp dir {}", path.display());
            }
        } else if fs::remove_file(&path).is_ok() {
            removed += 1;
        } else {
            log::warn!("failed to remove temp file {}", path.display());
        }
    }

    Ok(removed)
}

pub fn cleanup_stale_temp_dirs(base: &Path, retention: Duration) -> Result<usize> {
    cleanup_stale_temp_entries(base, &[RALPH_TEMP_PREFIX], retention)
}

pub fn cleanup_default_temp_dirs(retention: Duration) -> Result<usize> {
    let mut removed = 0usize;
    removed += cleanup_stale_temp_dirs(&ralph_temp_root(), retention)?;
    removed +=
        cleanup_stale_temp_entries(&std::env::temp_dir(), &[LEGACY_PROMPT_PREFIX], retention)?;
    Ok(removed)
}

pub fn create_ralph_temp_dir(label: &str) -> Result<tempfile::TempDir> {
    let base = ralph_temp_root();
    fs::create_dir_all(&base).with_context(|| format!("create temp dir {}", base.display()))?;
    let prefix = format!(
        "{prefix}{label}_",
        prefix = RALPH_TEMP_PREFIX,
        label = label.trim()
    );
    let dir = tempfile::Builder::new()
        .prefix(&prefix)
        .tempdir_in(&base)
        .with_context(|| format!("create temp dir in {}", base.display()))?;
    Ok(dir)
}

/// Environment variable to opt-in to raw (non-redacted) safeguard dumps.
const ENV_RAW_DUMP: &str = "RALPH_RAW_DUMP";

/// Writes a safeguard dump with redaction applied to sensitive content.
///
/// This is the recommended default for error dumps. Secrets like API keys,
/// bearer tokens, AWS keys, and SSH keys are masked before writing.
///
/// Returns the path to the written file.
pub fn safeguard_text_dump_redacted(label: &str, content: &str) -> Result<PathBuf> {
    use crate::redaction::redact_text;
    let redacted_content = redact_text(content);
    safeguard_text_dump_internal(label, &redacted_content, true)
}

/// Writes a safeguard dump with raw (non-redacted) content.
///
/// SECURITY WARNING: This function writes raw content that may contain secrets.
/// It requires explicit opt-in via either:
/// - Setting the `RALPH_RAW_DUMP=1` environment variable
/// - Passing `is_debug_mode=true` (e.g., when `--debug` flag is used)
///
/// If opt-in is not provided, this function returns an error.
/// For safe dumping, use `safeguard_text_dump_redacted` instead.
pub fn safeguard_text_dump(label: &str, content: &str, is_debug_mode: bool) -> Result<PathBuf> {
    let raw_dump_enabled = std::env::var(ENV_RAW_DUMP)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !raw_dump_enabled && !is_debug_mode {
        anyhow::bail!(
            "Raw safeguard dumps require explicit opt-in. \
             Set {}=1 or use --debug mode. \
             Consider using safeguard_text_dump_redacted() for safe dumping.",
            ENV_RAW_DUMP
        );
    }

    if raw_dump_enabled {
        log::warn!(
            "SECURITY: Writing raw safeguard dump ({}=1). Secrets may be written to disk.",
            ENV_RAW_DUMP
        );
    }

    safeguard_text_dump_internal(label, content, false)
}

fn safeguard_text_dump_internal(label: &str, content: &str, _is_redacted: bool) -> Result<PathBuf> {
    let temp_dir = create_ralph_temp_dir(label)?;
    let output_path = temp_dir.path().join("output.txt");
    fs::write(&output_path, content)
        .with_context(|| format!("write safeguard dump to {}", output_path.display()))?;

    // Persist the temp dir so it's not deleted when the TempDir object is dropped.
    let dir_path = temp_dir.keep();
    Ok(dir_path.join("output.txt"))
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    log::debug!("atomic write: {}", path.display());
    let dir = path
        .parent()
        .context("atomic write requires a parent directory")?;
    fs::create_dir_all(dir).with_context(|| format!("create directory {}", dir.display()))?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("create temp file in {}", dir.display()))?;
    tmp.write_all(contents).context("write temp file")?;
    tmp.flush().context("flush temp file")?;
    tmp.as_file().sync_all().context("sync temp file")?;

    match tmp.persist(path) {
        Ok(_) => {}
        Err(err) => {
            // Explicitly drop the temp file to ensure cleanup on persist failure.
            // PersistError contains both the error and the NamedTempFile handle;
            // we must extract and drop the file handle to prevent temp file leaks.
            let _temp_file = err.file;
            drop(_temp_file);
            return Err(err.error).with_context(|| format!("persist {}", path.display()));
        }
    }

    sync_dir_best_effort(dir);
    Ok(())
}

pub(crate) fn sync_dir_best_effort(dir: &Path) {
    #[cfg(unix)]
    {
        log::debug!("syncing directory: {}", dir.display());
        if let Ok(file) = fs::File::open(dir) {
            let _ = file.sync_all();
        }
    }

    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn safeguard_text_dump_redacted_masks_secrets() {
        let content = "API_KEY=sk-abc123xyz789\nAuthorization: Bearer secret_token_12345";
        let path = safeguard_text_dump_redacted("test_redacted", content).unwrap();

        let written = fs::read_to_string(&path).unwrap();

        assert!(
            !written.contains("sk-abc123xyz789"),
            "API key should be redacted"
        );
        assert!(
            !written.contains("secret_token_12345"),
            "Bearer token should be redacted"
        );
        assert!(
            written.contains("[REDACTED]"),
            "Should contain redaction marker"
        );

        // Cleanup
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn safeguard_text_dump_requires_opt_in_without_debug() {
        let _guard = env_lock().lock().expect("env lock");

        // Ensure env var is not set
        std::env::remove_var(ENV_RAW_DUMP);

        let content = "sensitive data";
        let result = safeguard_text_dump("test_raw", content, false);

        assert!(result.is_err(), "Raw dump should fail without opt-in");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("RALPH_RAW_DUMP"),
            "Error should mention env var"
        );
    }

    #[test]
    fn safeguard_text_dump_allows_raw_with_env_var() {
        let _guard = env_lock().lock().expect("env lock");

        std::env::set_var(ENV_RAW_DUMP, "1");

        let content = "raw secret data";
        let path = safeguard_text_dump("test_raw_env", content, false).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, content, "Raw content should be written unchanged");

        // Cleanup
        std::env::remove_var(ENV_RAW_DUMP);
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn safeguard_text_dump_allows_raw_with_debug_mode() {
        let _guard = env_lock().lock().expect("env lock");

        // Ensure env var is not set
        std::env::remove_var(ENV_RAW_DUMP);

        let content = "debug mode secret";
        let path = safeguard_text_dump("test_raw_debug", content, true).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(
            written, content,
            "Raw content should be written in debug mode"
        );

        // Cleanup
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn safeguard_text_dump_preserves_non_sensitive_content() {
        let content = "This is normal log output without secrets";
        let path = safeguard_text_dump_redacted("test_normal", content).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(
            written, content,
            "Non-sensitive content should be preserved"
        );

        // Cleanup
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn safeguard_text_dump_redacts_aws_keys() {
        let content = "AWS Access Key: AKIAIOSFODNN7EXAMPLE";
        let path = safeguard_text_dump_redacted("test_aws", content).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(
            !written.contains("AKIAIOSFODNN7EXAMPLE"),
            "AWS key should be redacted"
        );
        assert!(
            written.contains("[REDACTED]"),
            "Should contain redaction marker"
        );

        // Cleanup
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    fn safeguard_text_dump_redacts_ssh_keys() {
        let content = "SSH Key:\n-----BEGIN OPENSSH PRIVATE KEY-----\nabc123\n-----END OPENSSH PRIVATE KEY-----";
        let path = safeguard_text_dump_redacted("test_ssh", content).unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(
            !written.contains("abc123"),
            "SSH key content should be redacted"
        );
        assert!(
            written.contains("[REDACTED]"),
            "Should contain redaction marker"
        );

        // Cleanup
        let _ = fs::remove_file(&path);
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }

    #[test]
    #[cfg(unix)]
    fn write_atomic_cleans_up_temp_file_on_persist_failure() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("readonly");
        fs::create_dir(&target_dir).unwrap();

        // Create a file inside the directory first (so we have something to "persist to")
        // then make the directory read-only. This prevents new file creation/replacement.
        let existing_file = target_dir.join("existing.txt");
        fs::write(&existing_file, "existing content").unwrap();

        // Make directory read-only (removes write permission)
        let mut perms = fs::metadata(&target_dir).unwrap().permissions();
        perms.set_mode(0o555); // read + execute only
        fs::set_permissions(&target_dir, perms).unwrap();

        // Attempt to write to a new file in the read-only directory
        let target_file = target_dir.join("test.txt");
        let result = write_atomic(&target_file, b"test content");

        // Should fail due to permission denied
        assert!(
            result.is_err(),
            "write_atomic should fail in read-only directory"
        );

        // Should not leave temp files behind in the target directory
        let entries: Vec<_> = fs::read_dir(&target_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with(".") || name.starts_with("tmp") || name.starts_with("ralph")
            })
            .collect();
        assert!(
            entries.is_empty(),
            "Temp files should be cleaned up, found: {:?}",
            entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
        );

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&target_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_dir, perms).unwrap();
    }
}
