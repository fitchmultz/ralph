//! Runner retry policy + backoff.
//!
//! Responsibilities:
//! - Resolve `agent.runner_retry` config into a concrete policy.
//! - Compute exponential backoff with cap + jitter.
//!
//! Does not handle:
//! - Deciding whether an error is retryable (see `RunnerError::classify`).
//! - Actually executing runner invocations (see `runutil::execution`).

use std::time::Duration;

use crate::contracts::RunnerRetryConfig;

/// Concrete retry policy resolved from config.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RunnerRetryPolicy {
    /// Total attempts including initial (>= 1).
    pub max_attempts: u32,
    /// Base backoff duration.
    pub base_backoff: Duration,
    /// Exponential multiplier (>= 1.0).
    pub multiplier: f64,
    /// Maximum backoff cap.
    pub max_backoff: Duration,
    /// Jitter ratio in [0, 1].
    pub jitter_ratio: f64,
}

impl RunnerRetryPolicy {
    /// Create a policy from config, applying defaults for missing values.
    pub(crate) fn from_config(cfg: &RunnerRetryConfig) -> anyhow::Result<Self> {
        let max_attempts = cfg.max_attempts.unwrap_or(3).max(1);
        let base_backoff = Duration::from_millis(cfg.base_backoff_ms.unwrap_or(1000) as u64);
        let multiplier = cfg.multiplier.unwrap_or(2.0).max(1.0);
        let max_backoff = Duration::from_millis(cfg.max_backoff_ms.unwrap_or(30_000) as u64);
        let jitter_ratio = cfg.jitter_ratio.unwrap_or(0.2).clamp(0.0, 1.0);
        Ok(Self {
            max_attempts,
            base_backoff,
            multiplier,
            max_backoff,
            jitter_ratio,
        })
    }

    /// Returns true if retry is enabled (more than 1 attempt allowed).
    /// Kept as public API for future retry control flow extensions.
    #[allow(dead_code)]
    pub(crate) fn enabled(self) -> bool {
        self.max_attempts > 1
    }
}

impl Default for RunnerRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_backoff: Duration::from_millis(1000),
            multiplier: 2.0,
            max_backoff: Duration::from_millis(30_000),
            jitter_ratio: 0.2,
        }
    }
}

/// Trait for jitter random number generation (allows deterministic testing).
pub(crate) trait JitterRng {
    /// Generate a uniform random f64 in [lo, hi).
    fn uniform_f64(&mut self, lo: f64, hi: f64) -> f64;
}

/// Simple xorshift-based RNG for jitter (small, no extra deps).
pub(crate) struct SeededRng {
    state: u64,
}

impl SeededRng {
    /// Create a new RNG seeded from current time.
    pub(crate) fn new() -> Self {
        // Use a simple hash of current time for seed
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x123456789ABCDEF);
        Self { state: seed }
    }

    /// Create from a specific seed (for deterministic testing).
    #[cfg(test)]
    pub(crate) fn from_seed(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl JitterRng for SeededRng {
    fn uniform_f64(&mut self, lo: f64, hi: f64) -> f64 {
        let range = hi - lo;
        let fraction = (self.next_u64() as f64) / (u64::MAX as f64);
        lo + fraction * range
    }
}

/// Deterministic RNG for testing (always returns the same sequence).
#[cfg(test)]
pub(crate) struct FixedRng {
    values: Vec<f64>,
    index: usize,
}

#[cfg(test)]
impl FixedRng {
    pub(crate) fn new(values: Vec<f64>) -> Self {
        Self { values, index: 0 }
    }
}

#[cfg(test)]
impl JitterRng for FixedRng {
    fn uniform_f64(&mut self, lo: f64, hi: f64) -> f64 {
        let range = hi - lo;
        let fraction = self.values.get(self.index).copied().unwrap_or(0.5);
        self.index = (self.index + 1) % self.values.len().max(1);
        lo + fraction * range
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
    // Validate multiplier invariant at computation site
    if policy.multiplier < 1.0 {
        // Log warning and clamp to valid range
        log::warn!(
            "Invalid multiplier {} in retry policy, clamping to 1.0",
            policy.multiplier
        );
    }
    let multiplier = policy.multiplier.max(1.0);

    // retry_index starts at 1 for first retry delay
    let power_u32 = retry_index.saturating_sub(1);

    // Check for potential overflow before multiplication
    // Duration::MAX is about u64::MAX milliseconds
    let max_ms = u64::MAX as f64;
    let base_ms = policy.base_backoff.as_millis() as f64;

    // Calculate exponential with overflow detection
    // Check if power is too large to cast to i32 (i32::MAX = 2,147,483,647)
    // or if power > 1000 (any multiplier^1000 where multiplier >= 1.0 will be huge)
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

    // Final safety check before conversion
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
        format!("{:.1}s", secs)
    } else {
        format!("{}ms", duration.as_millis())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy() -> RunnerRetryPolicy {
        RunnerRetryPolicy {
            max_attempts: 3,
            base_backoff: Duration::from_millis(1000),
            multiplier: 2.0,
            max_backoff: Duration::from_millis(30_000),
            jitter_ratio: 0.0, // No jitter for deterministic tests
        }
    }

    #[test]
    fn backoff_exponential_growth() {
        let policy = test_policy();
        let mut rng = FixedRng::new(vec![0.0]);

        // First retry: 1000ms * 2^0 = 1000ms
        let d1 = compute_backoff(policy, 1, &mut rng);
        assert_eq!(d1.as_millis(), 1000);

        // Second retry: 1000ms * 2^1 = 2000ms
        let d2 = compute_backoff(policy, 2, &mut rng);
        assert_eq!(d2.as_millis(), 2000);

        // Third retry: 1000ms * 2^2 = 4000ms
        let d3 = compute_backoff(policy, 3, &mut rng);
        assert_eq!(d3.as_millis(), 4000);
    }

    #[test]
    fn backoff_respects_max_cap() {
        let policy = RunnerRetryPolicy {
            max_attempts: 10,
            base_backoff: Duration::from_millis(1000),
            multiplier: 10.0, // Fast growth
            max_backoff: Duration::from_millis(5000),
            jitter_ratio: 0.0,
        };
        let mut rng = FixedRng::new(vec![0.0]);

        // Even at high retry index, should be capped at max_backoff
        let d = compute_backoff(policy, 5, &mut rng);
        assert_eq!(d.as_millis(), 5000);
    }

    #[test]
    fn backoff_with_jitter() {
        let policy = RunnerRetryPolicy {
            max_attempts: 3,
            base_backoff: Duration::from_millis(1000),
            multiplier: 2.0,
            max_backoff: Duration::from_millis(30_000),
            jitter_ratio: 0.2, // 20% jitter
        };

        // Test with fixed RNG that returns 0.5 (middle of range)
        // With jitter_ratio=0.2, range is [-0.2, +0.2], middle is 0
        let mut rng = FixedRng::new(vec![0.5]);
        let d = compute_backoff(policy, 1, &mut rng);
        // Should be close to base backoff (1000ms) with no effective jitter at middle
        assert!(d.as_millis() >= 800 && d.as_millis() <= 1200);
    }

    #[test]
    fn backoff_jitter_bounds() {
        let policy = RunnerRetryPolicy {
            max_attempts: 3,
            base_backoff: Duration::from_millis(1000),
            multiplier: 2.0,
            max_backoff: Duration::from_millis(30_000),
            jitter_ratio: 0.2,
        };

        // Test with extreme RNG values
        let mut rng_min = FixedRng::new(vec![0.0]); // -0.2 jitter
        let d_min = compute_backoff(policy, 1, &mut rng_min);
        // 1000ms * (1 - 0.2) = 800ms
        assert_eq!(d_min.as_millis(), 800);

        let mut rng_max = FixedRng::new(vec![1.0]); // +0.2 jitter
        let d_max = compute_backoff(policy, 1, &mut rng_max);
        // 1000ms * (1 + 0.2) = 1200ms
        assert_eq!(d_max.as_millis(), 1200);
    }

    #[test]
    fn policy_from_config_defaults() {
        let cfg = RunnerRetryConfig::default();
        let policy = RunnerRetryPolicy::from_config(&cfg).unwrap();

        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.base_backoff.as_millis(), 1000);
        assert_eq!(policy.multiplier, 2.0);
        assert_eq!(policy.max_backoff.as_millis(), 30000);
        assert_eq!(policy.jitter_ratio, 0.2);
        assert!(policy.enabled());
    }

    #[test]
    fn policy_from_config_custom() {
        let cfg = RunnerRetryConfig {
            max_attempts: Some(5),
            base_backoff_ms: Some(500),
            multiplier: Some(1.5),
            max_backoff_ms: Some(10000),
            jitter_ratio: Some(0.1),
        };
        let policy = RunnerRetryPolicy::from_config(&cfg).unwrap();

        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.base_backoff.as_millis(), 500);
        assert_eq!(policy.multiplier, 1.5);
        assert_eq!(policy.max_backoff.as_millis(), 10000);
        assert_eq!(policy.jitter_ratio, 0.1);
    }

    #[test]
    fn policy_from_config_bounds() {
        let cfg = RunnerRetryConfig {
            max_attempts: Some(0),   // Should be clamped to 1
            multiplier: Some(0.5),   // Should be clamped to 1.0
            jitter_ratio: Some(1.5), // Should be clamped to 1.0
            ..Default::default()
        };
        let policy = RunnerRetryPolicy::from_config(&cfg).unwrap();

        assert_eq!(policy.max_attempts, 1);
        assert_eq!(policy.multiplier, 1.0);
        assert_eq!(policy.jitter_ratio, 1.0);
    }

    #[test]
    fn policy_enabled_only_when_multiple_attempts() {
        let enabled = RunnerRetryPolicy {
            max_attempts: 3,
            ..test_policy()
        };
        assert!(enabled.enabled());

        let disabled = RunnerRetryPolicy {
            max_attempts: 1,
            ..test_policy()
        };
        assert!(!disabled.enabled());
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration(Duration::from_secs(10)), "10.0s");
    }

    #[test]
    fn format_duration_millis() {
        assert_eq!(format_duration(Duration::from_millis(200)), "200ms");
        assert_eq!(format_duration(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn seeded_rng_produces_values() {
        let mut rng = SeededRng::from_seed(12345);
        let v1 = rng.uniform_f64(-0.2, 0.2);
        let v2 = rng.uniform_f64(-0.2, 0.2);
        // Values should be in range
        assert!((-0.2..=0.2).contains(&v1));
        assert!((-0.2..=0.2).contains(&v2));
        // Should produce different values
        assert_ne!(v1, v2);
    }

    #[test]
    fn backoff_handles_extremely_high_retry_index() {
        let policy = RunnerRetryPolicy {
            max_attempts: u32::MAX,
            base_backoff: Duration::from_millis(1000),
            multiplier: 2.0,
            max_backoff: Duration::from_millis(30_000),
            jitter_ratio: 0.0,
        };
        let mut rng = FixedRng::new(vec![0.0]);

        // Should not panic with very high retry index
        let d = compute_backoff(policy, u32::MAX, &mut rng);
        // Should be capped at max_backoff
        assert_eq!(d, Duration::from_millis(30_000));
    }

    #[test]
    fn backoff_handles_large_multiplier() {
        let policy = RunnerRetryPolicy {
            max_attempts: 10,
            base_backoff: Duration::from_millis(100),
            multiplier: 1000.0, // Very large multiplier
            max_backoff: Duration::from_secs(60),
            jitter_ratio: 0.0,
        };
        let mut rng = FixedRng::new(vec![0.0]);

        // Second retry would be 100 * 1000^1 = 100,000ms
        let d = compute_backoff(policy, 2, &mut rng);
        // Should be capped at max_backoff
        assert_eq!(d, Duration::from_secs(60));
    }

    #[test]
    fn backoff_handles_invalid_multiplier() {
        let policy = RunnerRetryPolicy {
            max_attempts: 3,
            base_backoff: Duration::from_millis(1000),
            multiplier: 0.5, // Invalid: < 1.0
            max_backoff: Duration::from_millis(10_000),
            jitter_ratio: 0.0,
        };
        let mut rng = FixedRng::new(vec![0.0]);

        // Should treat multiplier as 1.0 (no exponential growth)
        let d1 = compute_backoff(policy, 1, &mut rng);
        assert_eq!(d1, Duration::from_millis(1000));

        let d2 = compute_backoff(policy, 5, &mut rng);
        // With multiplier clamped to 1.0, all retries should be ~1000ms
        assert_eq!(d2, Duration::from_millis(1000));
    }

    #[test]
    fn backoff_respects_duration_max() {
        let policy = RunnerRetryPolicy {
            max_attempts: 100,
            base_backoff: Duration::from_millis(u64::MAX / 2),
            multiplier: 2.0,
            max_backoff: Duration::MAX, // Very large cap
            jitter_ratio: 0.0,
        };
        let mut rng = FixedRng::new(vec![0.0]);

        // Even with huge values, should not overflow
        let d = compute_backoff(policy, 2, &mut rng);
        // Should be capped at Duration::MAX via max_backoff
        assert!(d <= Duration::MAX);
    }
}
