//! Purpose: Regression coverage for the `crate::fsutil` facade surface.
//!
//! Responsibilities:
//! - Verify safeguard dump, atomic write, and temp-file behaviors remain unchanged.
//! - Exercise the facade re-exports so `crate::fsutil::*` stays the contract.
//! - Serialize environment-mutating tests that toggle raw-dump opt-in state.
//!
//! Scope:
//! - Unit tests for fsutil behavior only; broader integration coverage remains in `crates/ralph/tests/fsutil_test.rs`.
//!
//! Usage:
//! - Compiled only under `#[cfg(test)]` through `crate::fsutil::tests`.
//!
//! Invariants/Assumptions:
//! - Tests call the facade via `super::*` so re-exports remain stable.
//! - Environment-mutating tests serialize on a local mutex to avoid cross-test races.

use super::*;
use crate::constants::paths::ENV_RAW_DUMP;
use std::fs;
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
    unsafe { std::env::remove_var(ENV_RAW_DUMP) }

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

    unsafe { std::env::set_var(ENV_RAW_DUMP, "1") };

    let content = "raw secret data";
    let path = safeguard_text_dump("test_raw_env", content, false).unwrap();

    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(written, content, "Raw content should be written unchanged");

    // Cleanup
    unsafe { std::env::remove_var(ENV_RAW_DUMP) }
    let _ = fs::remove_file(&path);
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
}

#[test]
fn safeguard_text_dump_allows_raw_with_debug_mode() {
    let _guard = env_lock().lock().expect("env lock");

    // Ensure env var is not set
    unsafe { std::env::remove_var(ENV_RAW_DUMP) }

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
    let content =
        "SSH Key:\n-----BEGIN OPENSSH PRIVATE KEY-----\nabc123\n-----END OPENSSH PRIVATE KEY-----";
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

#[test]
fn create_ralph_temp_file_uses_ralph_prefix() {
    let temp = create_ralph_temp_file("test").unwrap();
    let name = temp.path().file_name().unwrap().to_string_lossy();
    assert!(
        name.starts_with("ralph_test_"),
        "temp file should have ralph prefix, got: {}",
        name
    );
    let parent = temp.path().parent().unwrap();
    assert!(
        parent.ends_with("ralph"),
        "temp file should be in ralph temp directory, got: {}",
        parent.display()
    );
}

#[test]
fn create_ralph_temp_file_is_cleaned_on_drop() {
    let path;
    {
        let temp = create_ralph_temp_file("test").unwrap();
        path = temp.path().to_path_buf();
        assert!(path.exists(), "temp file should exist while held");
    }
    // After drop, file should be removed
    assert!(!path.exists(), "temp file should be removed on drop");
}

#[test]
fn create_ralph_temp_file_accepts_content() {
    use std::io::Write;

    let mut temp = create_ralph_temp_file("test").unwrap();
    temp.write_all(b"test content").unwrap();
    temp.flush().unwrap();

    let content = fs::read_to_string(temp.path()).unwrap();
    assert_eq!(content, "test content");
}
