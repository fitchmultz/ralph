//! Redaction logic for sensitive data in strings, errors, and logs.

use std::fmt;

const REDACTED: &str = "[REDACTED]";
const MIN_ENV_VALUE_LEN: usize = 6;

/// A wrapper around `String` that applies redaction when displayed via `Display` or `Debug`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RedactedString(pub String);

impl From<String> for RedactedString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RedactedString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for RedactedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", redact_text(&self.0))
    }
}

impl fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "RedactedString({:#?})", redact_text(&self.0))
        } else {
            write!(f, "RedactedString({:?})", redact_text(&self.0))
        }
    }
}

/// A `log::Log` implementation that wraps another logger and redacts all log messages.
pub struct RedactedLogger {
    inner: Box<dyn log::Log>,
}

impl RedactedLogger {
    /// Creates a new `RedactedLogger` wrapping the given logger.
    pub fn new(inner: Box<dyn log::Log>) -> Self {
        Self { inner }
    }

    /// wraps the provided logger and sets it as the global logger.
    /// This is a convenience for `log::set_boxed_logger(Box::new(RedactedLogger::new(inner)))`.
    pub fn init(
        inner: Box<dyn log::Log>,
        max_level: log::LevelFilter,
    ) -> Result<(), log::SetLoggerError> {
        log::set_boxed_logger(Box::new(Self::new(inner)))?;
        log::set_max_level(max_level);
        Ok(())
    }
}

impl log::Log for RedactedLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            crate::debuglog::write_log_record(record);
            let redacted_msg = redact_text(&format!("{}", record.args()));
            self.inner.log(
                &log::Record::builder()
                    .args(format_args!("{}", redacted_msg))
                    .level(record.level())
                    .target(record.target())
                    .file(record.file())
                    .line(record.line())
                    .module_path(record.module_path())
                    .build(),
            );
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Macros for logging with immediate redaction.
/// Note: These are mostly redundant if `RedactedLogger` is used, but provide
/// explicit intent and can be used when the global logger is not wrapped.
#[macro_export]
macro_rules! rinfo {
    ($($arg:tt)+) => {
        log::info!("{}", $crate::redaction::redact_text(&format!($($arg)+)))
    }
}

#[macro_export]
macro_rules! rwarn {
    ($($arg:tt)+) => {
        log::warn!("{}", $crate::redaction::redact_text(&format!($($arg)+)))
    }
}

#[macro_export]
macro_rules! rerror {
    ($($arg:tt)+) => {
        log::error!("{}", $crate::redaction::redact_text(&format!($($arg)+)))
    }
}

#[macro_export]
macro_rules! rdebug {
    ($($arg:tt)+) => {
        log::debug!("{}", $crate::redaction::redact_text(&format!($($arg)+)))
    }
}

#[macro_export]
macro_rules! rtrace {
    ($($arg:tt)+) => {
        log::trace!("{}", $crate::redaction::redact_text(&format!($($arg)+)))
    }
}

pub fn redact_text(value: &str) -> String {
    if value.trim().is_empty() {
        return value.to_string();
    }

    let with_pairs = redact_key_value_pairs(value);
    let with_bearer = redact_bearer_tokens(&with_pairs);
    let with_aws = redact_aws_keys(&with_bearer);
    let with_ssh = redact_ssh_keys(&with_aws);
    let with_hex = redact_hex_tokens(&with_ssh);
    redact_sensitive_env_values(&with_hex)
}

pub fn looks_sensitive_env_key(key: &str) -> bool {
    let normalized = normalize_key(key);
    if normalized == "APIKEY" || normalized == "PRIVATEKEY" {
        return true;
    }
    for token in normalized.split(['_', '-']) {
        if token.is_empty() {
            continue;
        }
        if is_sensitive_token(token) {
            return true;
        }
    }
    false
}

pub fn is_path_like_env_key(key: &str) -> bool {
    matches!(
        normalize_key(key).as_str(),
        "CWD" | "HOME" | "OLDPWD" | "PATH" | "PWD" | "TEMP" | "TMP" | "TMPDIR"
    )
}

fn push_next_char(out: &mut String, text: &str, index: &mut usize) {
    debug_assert!(text.is_char_boundary(*index));
    let ch = text[*index..].chars().next().unwrap();
    out.push(ch);
    *index += ch.len_utf8();
}

fn redact_aws_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Look for AKIA...
        if i + 20 <= bytes.len() && &bytes[i..i + 4] == b"AKIA" {
            let mut all_caps_alphanum = true;
            for j in 0..16 {
                let b = bytes[i + 4 + j];
                if !(b.is_ascii_uppercase() || b.is_ascii_digit()) {
                    all_caps_alphanum = false;
                    break;
                }
            }
            if all_caps_alphanum {
                let word_boundary_start = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                let word_boundary_end =
                    i + 20 == bytes.len() || !bytes[i + 20].is_ascii_alphanumeric();

                if word_boundary_start && word_boundary_end {
                    out.push_str(REDACTED);
                    i += 20;
                    continue;
                }
            }
        }

        // Generic AWS secret lookahead (40 chars)
        // [0-9a-zA-Z/+=]{40}
        if i + 40 <= bytes.len() {
            let mut is_secret = true;
            for j in 0..40 {
                let b = bytes[i + j];
                if !(b.is_ascii_alphanumeric() || b == b'/' || b == b'+' || b == b'=') {
                    is_secret = false;
                    break;
                }
            }
            if is_secret {
                let word_boundary_start = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                let word_boundary_end =
                    i + 40 == bytes.len() || !bytes[i + 40].is_ascii_alphanumeric();

                if word_boundary_start && word_boundary_end {
                    // Check if it's near "secret" or "key" or "aws" or "akia"
                    // to reduce false positives if we wanted, but for now let's be aggressive.
                    out.push_str(REDACTED);
                    i += 40;
                    continue;
                }
            }
        }

        push_next_char(&mut out, text, &mut i);
    }
    out
}

fn redact_ssh_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < text.len() {
        if text[i..].starts_with("-----BEGIN") {
            if let Some(end_marker_pos) = text[i..].find("-----END") {
                if let Some(final_dash_pos) = text[i + end_marker_pos + 8..].find("-----") {
                    let total_end = i + end_marker_pos + 8 + final_dash_pos + 5;
                    out.push_str(REDACTED);
                    i = total_end;
                    continue;
                }
            }
        }
        push_next_char(&mut out, text, &mut i);
    }
    out
}

fn redact_hex_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i].is_ascii_hexdigit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            let len = i - start;
            if len >= 32 {
                let word_boundary_start = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
                let word_boundary_end = i == bytes.len() || !bytes[i].is_ascii_alphanumeric();

                if word_boundary_start && word_boundary_end {
                    out.push_str(REDACTED);
                    continue;
                }
            }
            out.push_str(&text[start..i]);
        } else {
            push_next_char(&mut out, text, &mut i);
        }
    }
    out
}

fn redact_key_value_pairs(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if !is_key_char(ch) {
            out.push(ch);
            i += 1;
            continue;
        }

        let start = i;
        let mut end = i;
        while end < chars.len() && is_key_char(chars[end]) {
            end += 1;
        }

        let key: String = chars[start..end].iter().collect();
        if looks_sensitive_label(&key) {
            let mut cursor = end;
            while cursor < chars.len() && chars[cursor].is_whitespace() && chars[cursor] != '\n' {
                cursor += 1;
            }
            if cursor < chars.len() && (chars[cursor] == ':' || chars[cursor] == '=') {
                cursor += 1;
                while cursor < chars.len() && chars[cursor].is_whitespace() && chars[cursor] != '\n'
                {
                    cursor += 1;
                }

                let value_start = cursor;
                let mut value_end = value_start;
                if value_start < chars.len()
                    && (chars[value_start] == '"' || chars[value_start] == '\'')
                {
                    let quote = chars[value_start];
                    value_end += 1;
                    while value_end < chars.len() && chars[value_end] != quote {
                        value_end += 1;
                    }
                    if value_end < chars.len() {
                        value_end += 1;
                    }
                } else {
                    while value_end < chars.len() && !chars[value_end].is_whitespace() {
                        value_end += 1;
                    }
                }

                out.extend(chars[i..value_start].iter());
                out.push_str(REDACTED);
                i = value_end;
                continue;
            }
        }

        out.extend(chars[i..end].iter());
        i = end;
    }

    out
}

fn redact_bearer_tokens(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let needle = "bearer ";
    let mut out = String::with_capacity(text.len());
    let mut index = 0;

    while let Some(pos) = lower[index..].find(needle) {
        let abs = index + pos;
        if abs > 0 {
            let prev = text.as_bytes()[abs - 1];
            if prev.is_ascii_alphanumeric() {
                let next_index = abs + 1;
                out.push_str(&text[index..next_index]);
                index = next_index;
                continue;
            }
        }

        let start = abs + needle.len();
        let bytes = text.as_bytes();
        let mut end = start;
        while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
            end += 1;
        }

        out.push_str(&text[index..start]);
        out.push_str(REDACTED);
        index = end;
    }

    out.push_str(&text[index..]);
    out
}

fn redact_sensitive_env_values(text: &str) -> String {
    let mut redacted = text.to_string();
    for (key, value) in std::env::vars() {
        if !looks_sensitive_env_key(&key) {
            continue;
        }
        if is_path_like_env_key(&key) {
            continue;
        }
        let trimmed = value.trim();
        if trimmed.len() < MIN_ENV_VALUE_LEN {
            continue;
        }
        redacted = redacted.replace(trimmed, REDACTED);
    }
    redacted
}

fn looks_sensitive_label(key: &str) -> bool {
    let normalized = normalize_key(key);
    if normalized == "APIKEY" || normalized == "PRIVATEKEY" {
        return true;
    }
    if normalized == "API_KEY" || normalized == "API-KEY" {
        return true;
    }
    if normalized == "PRIVATE_KEY" || normalized == "PRIVATE-KEY" {
        return true;
    }
    looks_sensitive_env_key(&normalized)
}

fn is_sensitive_token(token: &str) -> bool {
    let token_upper = token.to_ascii_uppercase();
    for base in ["KEY", "SECRET", "TOKEN", "PASSWORD"] {
        if token_upper == base {
            return true;
        }
        if let Some(suffix) = token_upper.strip_prefix(base) {
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn normalize_key(key: &str) -> String {
    key.trim().to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debuglog::{
        enable as enable_debug_log, reset_for_tests as reset_debug_log, test_lock as debug_lock,
    };
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
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
    fn redact_text_handles_non_ascii() {
        let input = "Read AGENTS.md — voila âêîö 你好";
        let output = redact_text(input);
        assert_eq!(output, input);
    }

    #[test]
    fn redact_text_masks_sensitive_env_values() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("API_TOKEN", "supersecretvalue");

        let input = "token is supersecretvalue";
        let output = redact_text(input);

        std::env::remove_var("API_TOKEN");

        assert!(!output.contains("supersecretvalue"));
        assert!(output.contains(REDACTED));
    }

    #[test]
    fn redact_text_leaves_non_sensitive_env_values() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("PATH", "/usr/bin");

        let input = "PATH=/usr/bin";
        let output = redact_text(input);

        std::env::remove_var("PATH");

        assert!(output.contains("/usr/bin"));
    }

    #[test]
    fn redact_text_masks_privatekey_env_value() {
        let _guard = env_lock().lock().expect("env lock");
        std::env::set_var("PRIVATEKEY", "supersecretkeyvalue");

        let input = "key is supersecretkeyvalue";
        let output = redact_text(input);

        std::env::remove_var("PRIVATEKEY");

        assert!(!output.contains("supersecretkeyvalue"));
        assert!(output.contains(REDACTED));
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
        reset_debug_log();
    }
}
