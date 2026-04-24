//! Parallel run statistics aggregation.
//!
//! Purpose:
//! - Parallel run statistics aggregation.
//!
//! Responsibilities:
//! - Track attempted, succeeded, and failed worker counts for one parallel run.
//! - Provide a small, intention-revealing API instead of threading raw counters through helpers.
//!
//! Not handled here:
//! - Worker state persistence.
//! - Notification or webhook delivery.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Each finished worker records exactly one terminal outcome.

#[derive(Debug, Clone, Default)]
pub(super) struct ParallelRunStats {
    attempted: usize,
    succeeded: usize,
    failed: usize,
}

impl ParallelRunStats {
    pub(super) fn record_success(&mut self) {
        self.attempted += 1;
        self.succeeded += 1;
    }

    pub(super) fn record_failure(&mut self) {
        self.attempted += 1;
        self.failed += 1;
    }

    pub(super) fn attempted(&self) -> usize {
        self.attempted
    }

    pub(super) fn succeeded(&self) -> usize {
        self.succeeded
    }

    pub(super) fn failed(&self) -> usize {
        self.failed
    }
}

#[cfg(test)]
mod tests {
    use super::ParallelRunStats;

    #[test]
    fn stats_accumulate_successes_and_failures() {
        let mut stats = ParallelRunStats::default();

        stats.record_success();
        stats.record_failure();
        stats.record_success();

        assert_eq!(stats.attempted(), 3);
        assert_eq!(stats.succeeded(), 2);
        assert_eq!(stats.failed(), 1);
    }
}
