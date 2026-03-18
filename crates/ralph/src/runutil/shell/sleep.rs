//! Purpose: Provide cancellation-aware retry waiting for managed operational loops.
//!
//! Responsibilities:
//! - Sleep for a requested duration when no cancellation handle exists.
//! - Poll a cancellation flag with bounded intervals when cooperative cancellation is enabled.
//!
//! Scope:
//! - Retry-wait cancellation handling only.
//!
//! Usage:
//! - Used by runtime orchestration that waits between managed retry attempts.
//!
//! Invariants/Assumptions:
//! - Cancellation must be observed promptly without busy-spinning.
//! - Poll intervals remain bounded by `MANAGED_RETRY_POLL_INTERVAL`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::types::RetryWaitError;
use crate::constants::timeouts;

pub(crate) fn sleep_with_cancellation(
    duration: Duration,
    cancellation: Option<&AtomicBool>,
) -> std::result::Result<(), RetryWaitError> {
    if cancellation.is_none() {
        thread::sleep(duration);
        return Ok(());
    }

    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if cancellation.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err(RetryWaitError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining.min(timeouts::MANAGED_RETRY_POLL_INTERVAL));
    }
    Ok(())
}
