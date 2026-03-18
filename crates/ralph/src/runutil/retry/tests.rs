//! Purpose: Regression coverage for runner retry policy and backoff helpers.
//!
//! Responsibilities:
//! - Verify policy defaulting and bounds enforcement.
//! - Guard exponential backoff, jitter, and overflow behavior.
//! - Cover fixed retry schedule semantics and duration formatting.
//!
//! Scope:
//! - Unit coverage for `crate::runutil::retry` only.
//!
//! Usage:
//! - Compiled via `#[cfg(test)]` from the retry facade.
//!
//! Invariants/Assumptions:
//! - Deterministic test RNG inputs must produce stable retry delays.
//! - Retry helpers should remain panic-free for extreme numeric inputs.

use std::time::Duration;

use crate::contracts::RunnerRetryConfig;

use super::rng::JitterRng;
use super::{FixedBackoffSchedule, RunnerRetryPolicy, SeededRng, compute_backoff, format_duration};

struct FixedRng {
    values: Vec<f64>,
    index: usize,
}

impl FixedRng {
    fn new(values: Vec<f64>) -> Self {
        Self { values, index: 0 }
    }
}

impl JitterRng for FixedRng {
    fn uniform_f64(&mut self, lo: f64, hi: f64) -> f64 {
        let range = hi - lo;
        let fraction = self.values.get(self.index).copied().unwrap_or(0.5);
        self.index = (self.index + 1) % self.values.len().max(1);
        lo + fraction * range
    }
}

fn test_policy() -> RunnerRetryPolicy {
    RunnerRetryPolicy {
        max_attempts: 3,
        base_backoff: Duration::from_millis(1000),
        multiplier: 2.0,
        max_backoff: Duration::from_millis(30_000),
        jitter_ratio: 0.0,
    }
}

#[test]
fn backoff_exponential_growth() {
    let policy = test_policy();
    let mut rng = FixedRng::new(vec![0.0]);

    let d1 = compute_backoff(policy, 1, &mut rng);
    assert_eq!(d1.as_millis(), 1000);

    let d2 = compute_backoff(policy, 2, &mut rng);
    assert_eq!(d2.as_millis(), 2000);

    let d3 = compute_backoff(policy, 3, &mut rng);
    assert_eq!(d3.as_millis(), 4000);
}

#[test]
fn backoff_respects_max_cap() {
    let policy = RunnerRetryPolicy {
        max_attempts: 10,
        base_backoff: Duration::from_millis(1000),
        multiplier: 10.0,
        max_backoff: Duration::from_millis(5000),
        jitter_ratio: 0.0,
    };
    let mut rng = FixedRng::new(vec![0.0]);

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
        jitter_ratio: 0.2,
    };

    let mut rng = FixedRng::new(vec![0.5]);
    let d = compute_backoff(policy, 1, &mut rng);
    assert!((800..=1200).contains(&d.as_millis()));
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

    let mut rng_min = FixedRng::new(vec![0.0]);
    let d_min = compute_backoff(policy, 1, &mut rng_min);
    assert_eq!(d_min.as_millis(), 800);

    let mut rng_max = FixedRng::new(vec![1.0]);
    let d_max = compute_backoff(policy, 1, &mut rng_max);
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
        max_attempts: Some(0),
        multiplier: Some(0.5),
        jitter_ratio: Some(1.5),
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
    assert!((-0.2..=0.2).contains(&v1));
    assert!((-0.2..=0.2).contains(&v2));
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

    let d = compute_backoff(policy, u32::MAX, &mut rng);
    assert_eq!(d, Duration::from_millis(30_000));
}

#[test]
fn backoff_handles_large_multiplier() {
    let policy = RunnerRetryPolicy {
        max_attempts: 10,
        base_backoff: Duration::from_millis(100),
        multiplier: 1000.0,
        max_backoff: Duration::from_secs(60),
        jitter_ratio: 0.0,
    };
    let mut rng = FixedRng::new(vec![0.0]);

    let d = compute_backoff(policy, 2, &mut rng);
    assert_eq!(d, Duration::from_secs(60));
}

#[test]
fn backoff_handles_invalid_multiplier() {
    let policy = RunnerRetryPolicy {
        max_attempts: 3,
        base_backoff: Duration::from_millis(1000),
        multiplier: 0.5,
        max_backoff: Duration::from_millis(10_000),
        jitter_ratio: 0.0,
    };
    let mut rng = FixedRng::new(vec![0.0]);

    let d1 = compute_backoff(policy, 1, &mut rng);
    assert_eq!(d1, Duration::from_millis(1000));

    let d2 = compute_backoff(policy, 5, &mut rng);
    assert_eq!(d2, Duration::from_millis(1000));
}

#[test]
fn backoff_respects_duration_max() {
    let policy = RunnerRetryPolicy {
        max_attempts: 100,
        base_backoff: Duration::from_millis(u64::MAX / 2),
        multiplier: 2.0,
        max_backoff: Duration::MAX,
        jitter_ratio: 0.0,
    };
    let mut rng = FixedRng::new(vec![0.0]);

    let d = compute_backoff(policy, 2, &mut rng);
    assert!(d <= Duration::MAX);
}

#[test]
fn fixed_schedule_reuses_last_delay() {
    let schedule = FixedBackoffSchedule::from_millis(&[100, 250]);

    assert_eq!(schedule.delay_for_retry(0), Duration::from_millis(100));
    assert_eq!(schedule.delay_for_retry(1), Duration::from_millis(250));
    assert_eq!(schedule.delay_for_retry(5), Duration::from_millis(250));
}
