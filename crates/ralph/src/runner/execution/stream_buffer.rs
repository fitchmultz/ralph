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

/// Append text to buffer, truncating older content if MAX_BUFFER_SIZE would be exceeded.
/// Returns true if truncation occurred for the first time, false otherwise.
pub(super) fn append_to_buffer(
    buffer: &mut String,
    text: &str,
    exceeded_logged: &mut bool,
) -> bool {
    if buffer.len() + text.len() > MAX_BUFFER_SIZE {
        let should_log = !*exceeded_logged;
        if should_log {
            log::warn!(
                "Runner output buffer exceeded {}MB limit; truncating older content",
                MAX_BUFFER_SIZE / (1024 * 1024)
            );
            *exceeded_logged = true;
        }
        if text.len() >= MAX_BUFFER_SIZE {
            buffer.clear();
            let start = text.floor_char_boundary(text.len() - MAX_BUFFER_SIZE);
            buffer.push_str(&text[start..]);
        } else {
            let keep_from = buffer.len() + text.len() - MAX_BUFFER_SIZE;
            let start = buffer.floor_char_boundary(keep_from);
            let remaining = buffer.split_off(start);
            *buffer = remaining;
            buffer.push_str(text);
        }
        should_log
    } else {
        buffer.push_str(text);
        false
    }
}
