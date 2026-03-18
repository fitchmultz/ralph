//! Purpose: Resolve configured runner retry settings into a concrete policy.
//!
//! Responsibilities:
//! - Define the concrete retry policy used by runner orchestration.
//! - Apply schema/default bounds when materializing policy values from config.
//!
//! Scope:
//! - Policy shape and config-to-policy normalization only.
//!
//! Usage:
//! - Used by runner execution paths before computing retry delays.
//!
//! Invariants/Assumptions:
//! - `max_attempts` is always at least 1.
//! - `multiplier` stays at or above 1.0 and `jitter_ratio` stays within [0, 1].

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
