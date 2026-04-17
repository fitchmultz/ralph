//! Purpose: Preserve regression coverage for text redaction, env-key
//! detection, and redacted logging after the facade split.
//!
//! Responsibilities:
//! - Verify secret-shaped substrings are redacted while safe text remains.
//! - Verify sensitive env-key detection and path-like exclusions.
//! - Verify `RedactedLogger` redacts terminal output while raw debug logging
//!   remains unchanged.
//!
//! Scope:
//! - Redaction-specific behavior only; downstream callers stay covered in their
//!   own modules.
//!
//! Usage:
//! - Runs as the `redaction` unit test suite.
//!
//! Invariants/Assumptions:
//! - Assertions remain aligned with the former monolithic redaction tests.
//! - Public redaction API semantics remain unchanged.

use std::sync::{Mutex, OnceLock};

use tempfile::tempdir;

use crate::constants::defaults::REDACTED;
use crate::debuglog::{
    enable as enable_debug_log, reset_for_tests as reset_debug_log, test_lock as debug_lock,
};

use super::{RedactedLogger, is_path_like_env_key, looks_sensitive_env_key, redact_text};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        unsafe { std::env::set_var(key, value) };
        Self { key }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        unsafe { std::env::remove_var(self.key) };
    }
}

#[test]
fn looks_sensitive_env_key_matches_expected_values() {
    let cases = [
        ("API_KEY", true),
        ("password", true),
        ("auth-token", true),
        ("TOKEN1", true),
        ("  secret  ", true),
        ("PATH", false),
        ("HOME", false),
        ("SHELL", false),
        ("MONKEY", false),
        ("PRIVATEKEY", true),
        ("APIKEY", true),
    ];

    for (key, expected) in cases {
        assert_eq!(looks_sensitive_env_key(key), expected, "key={key}");
    }
}

#[test]
fn is_path_like_env_key_matches_expected_values() {
    let cases = [
        ("PATH", true),
        ("HOME", true),
        ("TMPDIR", true),
        ("  pwd  ", true),
        ("SHELL", false),
        ("PATH_INFO", false),
    ];

    for (key, expected) in cases {
        assert_eq!(is_path_like_env_key(key), expected, "key={key}");
    }
}

#[test]
fn redact_text_masks_key_value_pairs() {
    let input = "API_KEY=abc12345 token:xyz98765 password = hunter2";
    let output = redact_text(input);
    assert!(!output.contains("abc12345"));
    assert!(!output.contains("xyz98765"));
    assert!(!output.contains("hunter2"));
    assert!(output.contains("API_KEY=[REDACTED]"));
    assert!(output.contains("token:[REDACTED]"));
    assert!(output.contains("password = [REDACTED]"));
}

#[test]
fn redact_text_masks_bearer_tokens() {
    let input = "Authorization: Bearer abcdef123456";
    let output = redact_text(input);
    assert!(!output.contains("abcdef123456"));
    assert!(output.contains("Bearer [REDACTED]"));
}

#[test]
fn redact_text_preserves_sha_like_hex_identifiers() {
    let git_sha = "0123456789abcdef0123456789abcdef01234567";
    let sha256 = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let input = format!("commit {git_sha} artifact sha256:{sha256}");

    let output = redact_text(&input);

    assert_eq!(output, input);
}

#[test]
fn redact_text_masks_sensitive_context_hex_tokens() {
    let session_token = "abcdef0123456789abcdef0123456789";
    let webhook_signature = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let input =
        format!("session token {session_token}; X-Ralph-Signature: sha256={webhook_signature}");

    let output = redact_text(&input);

    assert!(!output.contains(session_token));
    assert!(!output.contains(webhook_signature));
    assert!(output.contains("session token [REDACTED]"));
    assert!(output.contains("X-Ralph-Signature: sha256=[REDACTED]"));
}

#[test]
fn redact_text_masks_unlabeled_very_long_hex_tokens() {
    let secret_blob = concat!(
        "abcdef0123456789abcdef0123456789",
        "abcdef0123456789abcdef0123456789",
        "abcdef0123456789abcdef0123456789"
    );
    let input = format!("raw blob {secret_blob}");

    let output = redact_text(&input);

    assert!(!output.contains(secret_blob));
    assert!(output.contains("raw blob [REDACTED]"));
}

#[test]
fn redact_text_preserves_hex_sha_while_masking_aws_secret_access_key() {
    let git_sha = "0123456789abcdef0123456789abcdef01234567";
    let aws_secret = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let input = format!("commit {git_sha} aws {aws_secret}");

    let output = redact_text(&input);

    assert!(output.contains(git_sha));
    assert!(!output.contains(aws_secret));
    assert!(output.contains("aws [REDACTED]"));
}

#[test]
fn redact_text_handles_non_ascii() {
    let input = "Read AGENTS.md — voila âêîö 你好";
    let output = redact_text(input);
    assert_eq!(output, input);
}

#[test]
fn redact_text_masks_sensitive_env_values() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::set_var("API_TOKEN", "supersecretvalue") };

    let input = "token is supersecretvalue";
    let output = redact_text(input);

    unsafe { std::env::remove_var("API_TOKEN") };

    assert!(!output.contains("supersecretvalue"));
    assert!(output.contains(REDACTED));
}

#[test]
fn redact_text_leaves_non_sensitive_env_values() {
    let _guard = env_lock().lock().expect("env lock");
    let key = "RALPH_NON_SENSITIVE_ENV";
    let value = "visible_plain_value";
    unsafe { std::env::set_var(key, value) };

    let input = "value is visible_plain_value";
    let output = redact_text(input);

    unsafe { std::env::remove_var(key) };

    assert!(output.contains(value));
}

#[test]
fn redact_text_masks_privatekey_env_value() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::set_var("PRIVATEKEY", "supersecretkeyvalue") };

    let input = "key is supersecretkeyvalue";
    let output = redact_text(input);

    unsafe { std::env::remove_var("PRIVATEKEY") };

    assert!(!output.contains("supersecretkeyvalue"));
    assert!(output.contains(REDACTED));
}

#[test]
fn redact_text_reads_latest_sensitive_env_values_without_manual_cache_clear() {
    let _guard = env_lock().lock().expect("env lock");
    unsafe { std::env::set_var("API_TOKEN", "initialsecretvalue") };
    let first = redact_text("token is initialsecretvalue");
    unsafe { std::env::set_var("API_TOKEN", "updatedsecretvalue") };
    let second = redact_text("token is updatedsecretvalue");
    unsafe { std::env::remove_var("API_TOKEN") };

    assert!(!first.contains("initialsecretvalue"));
    assert!(!second.contains("updatedsecretvalue"));
    assert!(first.contains(REDACTED));
    assert!(second.contains(REDACTED));
}

#[test]
fn redact_text_writes_safe_debug_metadata_for_fired_classes() {
    let _debug_guard = debug_lock().lock().expect("debug log lock");
    let _env_guard = env_lock().lock().expect("env lock");
    reset_debug_log();
    let dir = tempdir().expect("tempdir");
    enable_debug_log(dir.path()).expect("enable debug log");
    let _api_token = EnvVarGuard::set("API_TOKEN", "supersecretenvvalue");
    let contextual_hex = "abcdef0123456789abcdef0123456789";
    let structural_secret = "structuralsecret";

    let input = format!(
        "API_KEY={structural_secret} session token {contextual_hex} env supersecretenvvalue"
    );
    let output = redact_text(&input);

    assert!(!output.contains(structural_secret));
    assert!(!output.contains(contextual_hex));
    assert!(!output.contains("supersecretenvvalue"));

    let debug_log = dir.path().join(".ralph/logs/debug.log");
    let contents = std::fs::read_to_string(&debug_log).expect("read log");
    let metadata_lines: Vec<&str> = contents
        .lines()
        .filter(|line| line.starts_with("[REDACTION]"))
        .collect();
    assert!(
        metadata_lines.contains(&"[REDACTION] classes=structural,hex,env-value material=omitted"),
        "metadata: {metadata_lines:?}"
    );
    for line in metadata_lines {
        assert!(!line.contains(structural_secret), "metadata: {line}");
        assert!(!line.contains(contextual_hex), "metadata: {line}");
        assert!(!line.contains("supersecretenvvalue"), "metadata: {line}");
    }
    assert!(
        !contents.contains(structural_secret),
        "debug log: {contents}"
    );
    assert!(!contents.contains(contextual_hex), "debug log: {contents}");
    assert!(
        !contents.contains("supersecretenvvalue"),
        "debug log: {contents}"
    );
    reset_debug_log();
}

struct MockLogger {
    last_msg: std::sync::Arc<std::sync::Mutex<String>>,
}

impl log::Log for MockLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let mut lock = self.last_msg.lock().unwrap();
        *lock = format!("{}", record.args());
    }

    fn flush(&self) {}
}

#[test]
fn redacted_logger_masks_output() {
    let last_msg = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mock = Box::new(MockLogger {
        last_msg: last_msg.clone(),
    });

    let wrapper = RedactedLogger::new(mock);

    let record = log::Record::builder()
        .args(format_args!("Connecting with API_KEY=secret123"))
        .level(log::Level::Info)
        .build();

    use log::Log;
    wrapper.log(&record);

    let msg = last_msg.lock().unwrap();
    assert!(!msg.contains("secret123"));
    assert!(msg.contains("API_KEY=[REDACTED]"));
}

#[test]
fn redacted_logger_writes_raw_log_to_debug_log() {
    let _guard = debug_lock().lock().expect("debug log lock");
    reset_debug_log();
    let dir = tempdir().expect("tempdir");
    enable_debug_log(dir.path()).expect("enable debug log");

    let last_msg = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mock = Box::new(MockLogger {
        last_msg: last_msg.clone(),
    });

    let wrapper = RedactedLogger::new(mock);

    let record = log::Record::builder()
        .args(format_args!("Connecting with API_KEY=secret123"))
        .level(log::Level::Info)
        .build();

    use log::Log;
    wrapper.log(&record);

    let debug_log = dir.path().join(".ralph/logs/debug.log");
    let contents = std::fs::read_to_string(&debug_log).expect("read log");
    assert!(contents.contains("API_KEY=secret123"), "log: {contents}");
    let metadata_lines: Vec<&str> = contents
        .lines()
        .filter(|line| line.starts_with("[REDACTION]"))
        .collect();
    assert!(
        metadata_lines.contains(&"[REDACTION] classes=structural material=omitted"),
        "metadata: {metadata_lines:?}"
    );
    for line in metadata_lines {
        assert!(!line.contains("secret123"), "metadata: {line}");
    }
    reset_debug_log();
}
