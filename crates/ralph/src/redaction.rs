const REDACTED: &str = "[REDACTED]";
const MIN_ENV_VALUE_LEN: usize = 6;

pub fn redact_text(value: &str) -> String {
    if value.trim().is_empty() {
        return value.to_string();
    }

    let with_pairs = redact_key_value_pairs(value);
    let with_bearer = redact_bearer_tokens(&with_pairs);
    redact_sensitive_env_values(&with_bearer)
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
    use std::sync::{Mutex, OnceLock};

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
}
