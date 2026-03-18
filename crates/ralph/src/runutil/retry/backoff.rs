//! Purpose: Compute retry delays and format retry timing output.
//!
//! Responsibilities:
//! - Provide fixed retry schedules for orchestration loops.
//! - Compute exponential backoff with cap, jitter, and overflow safety.
//! - Format retry delays for user-facing status messages.
//!
//! Scope:
//! - Backoff math and retry-delay formatting only.
//!
//! Usage:
//! - Used by runner execution orchestration and parallel-integration retry flows.
//!
//! Invariants/Assumptions:
//! - Retry delay growth must never overflow duration conversion.
//! - Jitter remains bounded by the configured jitter ratio.
//! - Fixed schedules reuse their final delay once explicit entries are exhausted.

use std::time::Duration;

use super::policy::RunnerRetryPolicy;
use super::rng::JitterRng;

/// Fixed retry schedule backed by an explicit list of delays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FixedBackoffSchedule {
    delays: Vec<Duration>,
}

impl FixedBackoffSchedule {
    /// Construct a schedule from millisecond delays.
    pub(crate) fn from_millis(delays_ms: &[u64]) -> Self {
        Self {
            delays: delays_ms
                .iter()
                .copied()
                .map(Duration::from_millis)
                .collect(),
        }
    }

    /// Returns the delay for the zero-based retry index, reusing the final delay once exhausted.
    pub(crate) fn delay_for_retry(&self, retry_index: usize) -> Duration {
        self.delays
            .get(retry_index)
            .copied()
            .or_else(|| self.delays.last().copied())
            .unwrap_or_default()
    }
}

/// Compute backoff duration for a given retry attempt.
///
/// - `retry_index`: 1 for first retry, 2 for second, etc.
/// - Returns: Duration to wait before next attempt.
pub(crate) fn compute_backoff(
    policy: RunnerRetryPolicy,
    retry_index: u32,
    rng: &mut impl JitterRng,
) -> Duration {
    if policy.multiplier < 1.0 {
        log::warn!(
            "Invalid multiplier {} in retry policy, clamping to 1.0",
            policy.multiplier
        );
    }
    let multiplier = policy.multiplier.max(1.0);
    let power_u32 = retry_index.saturating_sub(1);
    let max_ms = u64::MAX as f64;
    let base_ms = policy.base_backoff.as_millis() as f64;

    let exp_ms = if power_u32 > i32::MAX as u32 || power_u32 > 1000 {
        max_ms
    } else {
        let power = power_u32 as i32;
        let multiplier_pow = multiplier.powi(power);
        let product = base_ms * multiplier_pow;

        if product.is_infinite() || product.is_nan() || product > max_ms {
            max_ms
        } else {
            product
        }
    };

    let capped_ms = exp_ms.min(policy.max_backoff.as_millis() as f64).max(0.0);
    let jitter = if policy.jitter_ratio <= 0.0 {
        0.0
    } else {
        rng.uniform_f64(-policy.jitter_ratio, policy.jitter_ratio)
    };
    let ms = (capped_ms * (1.0 + jitter)).max(0.0);
    let ms_u64 = if ms > u64::MAX as f64 {
        u64::MAX
    } else {
        ms.round() as u64
    };

    Duration::from_millis(ms_u64)
}

/// Format a duration in a human-readable way (e.g., "1.5s", "200ms").
pub(crate) fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs >= 1.0 {
        format!("{secs:.1}s")
    } else {
        format!("{}ms", duration.as_millis())
    }
}
