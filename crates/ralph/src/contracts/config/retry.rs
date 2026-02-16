//! Runner retry/backoff configuration for transient failure handling.
//!
//! Responsibilities:
//! - Define retry config struct and merge behavior.
//!
//! Not handled here:
//! - Actual retry logic (see runner invocation modules).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Runner retry/backoff configuration for transient failure handling.
///
/// Controls automatic retry behavior when runner invocations fail with
/// transient errors (rate limits, temporary unavailability, network issues).
/// Distinct from webhook retry settings (`agent.webhook.retry_*`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RunnerRetryConfig {
    /// Total attempts (including initial). Default: 3.
    #[schemars(range(min = 1, max = 20))]
    pub max_attempts: Option<u32>,

    /// Base backoff in milliseconds. Default: 1000.
    #[schemars(range(min = 0, max = 600_000))]
    pub base_backoff_ms: Option<u32>,

    /// Exponential multiplier. Default: 2.0.
    #[schemars(range(min = 1.0, max = 10.0))]
    pub multiplier: Option<f64>,

    /// Max backoff cap in milliseconds. Default: 30000.
    #[schemars(range(min = 0, max = 600_000))]
    pub max_backoff_ms: Option<u32>,

    /// Jitter ratio in [0,1]. Default: 0.2 (20% variance).
    #[schemars(range(min = 0.0, max = 1.0))]
    pub jitter_ratio: Option<f64>,
}

impl RunnerRetryConfig {
    /// Leaf-wise merge: other.Some overrides self, other.None preserves self
    pub fn merge_from(&mut self, other: Self) {
        if other.max_attempts.is_some() {
            self.max_attempts = other.max_attempts;
        }
        if other.base_backoff_ms.is_some() {
            self.base_backoff_ms = other.base_backoff_ms;
        }
        if other.multiplier.is_some() {
            self.multiplier = other.multiplier;
        }
        if other.max_backoff_ms.is_some() {
            self.max_backoff_ms = other.max_backoff_ms;
        }
        if other.jitter_ratio.is_some() {
            self.jitter_ratio = other.jitter_ratio;
        }
    }
}
