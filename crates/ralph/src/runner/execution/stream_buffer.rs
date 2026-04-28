//! Shared bounded-buffer helpers for runner stream processing.
//!
//! Purpose:
//! - Shared bounded-buffer helpers for runner stream processing.
//!
//! Responsibilities:
//! - Enforce the global runner output buffer limit.
//! - Provide one truncation implementation for raw and JSON readers.
//!
//! Non-scope:
//! - Stream IO or JSON parsing.
//! - Terminal rendering or output handlers.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Callers hold any external synchronization around the target buffer.

use crate::constants::buffers::MAX_BUFFER_SIZE;

/// Append text to a bounded runner-output buffer, truncating older content when needed.
pub(super) fn append_to_buffer(buffer: &mut String, text: &str) {
    if buffer.len() + text.len() > MAX_BUFFER_SIZE {
        if text.len() >= MAX_BUFFER_SIZE {
            buffer.clear();
            let start = text.ceil_char_boundary(text.len() - MAX_BUFFER_SIZE);
            buffer.push_str(&text[start..]);
        } else {
            let keep_from = buffer.len() + text.len() - MAX_BUFFER_SIZE;
            let start = buffer.ceil_char_boundary(keep_from);
            let remaining = buffer.split_off(start);
            *buffer = remaining;
            buffer.push_str(text);
        }
    } else {
        buffer.push_str(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_to_buffer_stays_bounded_across_repeated_truncation() {
        let mut buffer = "a".repeat(MAX_BUFFER_SIZE - 4);

        append_to_buffer(&mut buffer, "123456");
        assert_eq!(buffer.len(), MAX_BUFFER_SIZE);
        assert!(buffer.ends_with("123456"));

        append_to_buffer(&mut buffer, "zz");
        assert_eq!(buffer.len(), MAX_BUFFER_SIZE);
        assert!(buffer.ends_with("zz"));
    }

    #[test]
    fn append_to_buffer_oversized_chunk_keeps_latest_utf8_tail() {
        let mut buffer = "old-prefix".to_string();
        let oversized = format!("{}t", "😀".repeat(MAX_BUFFER_SIZE / 4));

        append_to_buffer(&mut buffer, &oversized);
        assert!(buffer.len() <= MAX_BUFFER_SIZE);
        assert!(buffer.ends_with('t'));
        assert!(!buffer.contains("old-prefix"));
    }
}
