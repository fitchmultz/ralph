//! Purpose: Provide jitter RNG abstractions for retry backoff computation.
//!
//! Responsibilities:
//! - Define the minimal RNG trait that backoff computation depends on.
//! - Provide the production seeded RNG used by retry orchestration.
//!
//! Scope:
//! - Jitter random-number generation only.
//!
//! Usage:
//! - Consumed by `backoff.rs` and retry tests.
//!
//! Invariants/Assumptions:
//! - Generated values must honor the requested `[lo, hi)` interval.
//! - The production RNG stays dependency-light and deterministic under explicit test seeds.

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
