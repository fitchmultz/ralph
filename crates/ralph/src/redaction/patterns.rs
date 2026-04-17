//! Purpose: Apply secret redaction patterns to arbitrary text.
//!
//! Responsibilities:
//! - Redact known secret-shaped substrings such as key-value pairs, bearer
//!   tokens, AWS tokens, SSH blocks, high-risk hex strings, and sensitive env
//!   values.
//! - Emit safe class-level debug metadata when redaction changes text.
//! - Preserve non-secret text, including non-ASCII content.
//!
//! Scope:
//! - Text transformation only; environment-key detection and logging wrappers
//!   live in sibling modules.
//!
//! Usage:
//! - Called anywhere Ralph must render user-visible output safely.
//!
//! Invariants/Assumptions:
//! - Empty and whitespace-only strings round-trip unchanged.
//! - Redaction order remains key/value → bearer → AWS → SSH → hex → env-value.

use crate::constants::defaults::REDACTED;

use super::env::{get_sensitive_env_values, looks_sensitive_label};

const MIN_CONTEXTUAL_HEX_TOKEN_LEN: usize = 32;
const MIN_UNLABELED_HEX_TOKEN_LEN: usize = 96;
const HEX_CONTEXT_WINDOW: usize = 80;

pub fn redact_text(value: &str) -> String {
    if value.trim().is_empty() {
        return value.to_string();
    }

    let mut telemetry = RedactionTelemetry::default();

    let with_pairs = redact_key_value_pairs(value);
    telemetry.record_structural_change(value, &with_pairs);
    let with_bearer = redact_bearer_tokens(&with_pairs);
    telemetry.record_structural_change(&with_pairs, &with_bearer);
    let with_aws = redact_aws_keys(&with_bearer);
    telemetry.record_structural_change(&with_bearer, &with_aws);
    let with_ssh = redact_ssh_keys(&with_aws);
    telemetry.record_structural_change(&with_aws, &with_ssh);
    let with_hex = redact_hex_tokens(&with_ssh);
    telemetry.record_hex_change(&with_ssh, &with_hex);
    let redacted = redact_sensitive_env_values(&with_hex);
    telemetry.record_env_value_change(&with_hex, &redacted);
    emit_redaction_telemetry(telemetry);
    redacted
}

#[derive(Clone, Copy, Default)]
struct RedactionTelemetry {
    structural: bool,
    hex: bool,
    env_value: bool,
}

impl RedactionTelemetry {
    fn record_structural_change(&mut self, before: &str, after: &str) {
        self.structural |= before != after;
    }

    fn record_hex_change(&mut self, before: &str, after: &str) {
        self.hex |= before != after;
    }

    fn record_env_value_change(&mut self, before: &str, after: &str) {
        self.env_value |= before != after;
    }

    fn any(self) -> bool {
        self.structural || self.hex || self.env_value
    }
}

fn emit_redaction_telemetry(telemetry: RedactionTelemetry) {
    if !telemetry.any() {
        return;
    }

    crate::debuglog::with_debug_log(|log| {
        let mut classes = Vec::with_capacity(3);
        if telemetry.structural {
            classes.push("structural");
        }
        if telemetry.hex {
            classes.push("hex");
        }
        if telemetry.env_value {
            classes.push("env-value");
        }
        let _ = log.write(&format!(
            "[REDACTION] classes={} material=omitted\n",
            classes.join(",")
        ));
    });
}

fn push_next_char(out: &mut String, text: &str, index: &mut usize) {
    debug_assert!(text.is_char_boundary(*index));
    if let Some(ch) = text[*index..].chars().next() {
        out.push(ch);
        *index += ch.len_utf8();
    } else {
        *index += 1;
    }
}

fn redact_aws_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
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

        if i + 40 <= bytes.len() {
            let mut is_secret = true;
            let mut has_non_hex_secret_char = false;
            for j in 0..40 {
                let b = bytes[i + j];
                if !(b.is_ascii_alphanumeric() || b == b'/' || b == b'+' || b == b'=') {
                    is_secret = false;
                    break;
                }
                if !b.is_ascii_hexdigit() {
                    has_non_hex_secret_char = true;
                }
            }
            if is_secret && has_non_hex_secret_char {
                let word_boundary_start = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
                let word_boundary_end =
                    i + 40 == bytes.len() || !bytes[i + 40].is_ascii_alphanumeric();

                if word_boundary_start && word_boundary_end {
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
        if text[i..].starts_with("-----BEGIN")
            && let Some(end_marker_pos) = text[i..].find("-----END")
            && let Some(final_dash_pos) = text[i + end_marker_pos + 8..].find("-----")
        {
            let total_end = i + end_marker_pos + 8 + final_dash_pos + 5;
            out.push_str(REDACTED);
            i = total_end;
            continue;
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
            if should_redact_hex_token(text, start, i) {
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

fn should_redact_hex_token(text: &str, start: usize, end: usize) -> bool {
    let len = end - start;
    len >= MIN_UNLABELED_HEX_TOKEN_LEN
        || (len >= MIN_CONTEXTUAL_HEX_TOKEN_LEN && has_sensitive_hex_context(text, start))
}

fn has_sensitive_hex_context(text: &str, token_start: usize) -> bool {
    let mut context_start = token_start.saturating_sub(HEX_CONTEXT_WINDOW);
    while !text.is_char_boundary(context_start) {
        context_start += 1;
    }

    let context = text[context_start..token_start]
        .trim_end_matches(|ch: char| ch.is_ascii_whitespace() || ch == '"' || ch == '\'');
    if context.is_empty() {
        return false;
    }

    let normalized = normalize_hex_context(context);
    normalized
        .split_whitespace()
        .rev()
        .take(4)
        .any(is_sensitive_hex_context_word)
}

fn normalize_hex_context(context: &str) -> String {
    context
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect()
}

fn is_sensitive_hex_context_word(word: &str) -> bool {
    matches!(
        word,
        "auth"
            | "authorization"
            | "bearer"
            | "credential"
            | "credentials"
            | "hmac"
            | "key"
            | "password"
            | "passwd"
            | "secret"
            | "signature"
            | "signing"
            | "token"
            | "webhook"
    )
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
    let sensitive_values = get_sensitive_env_values();
    if sensitive_values.is_empty() {
        return text.to_string();
    }
    let mut redacted = text.to_string();
    for value in &sensitive_values {
        redacted = redacted.replace(value.as_str(), REDACTED);
    }
    redacted
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}
