//! Capacity counters for the parallel run loop.
//!
//! Purpose:
//! - Capacity counters for the parallel run loop.
//!
//! Responsibilities:
//! - Decide whether more tasks may start given `--max-tasks` and in-flight guard counts.
//!
//! Not handled here:
//! - Task selection or queue locking.
//! - Persisted worker history (state file is not authoritative for active capacity).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - In-flight worker count comes from the cleanup guard, not from `ParallelStateFile` alone.

use super::state;

pub(crate) fn effective_active_worker_count(
    _state_file: &state::ParallelStateFile,
    guard_in_flight_len: usize,
) -> usize {
    guard_in_flight_len
}

pub(crate) fn initial_tasks_started(_state_file: &state::ParallelStateFile) -> u32 {
    0
}

pub(crate) fn can_start_more_tasks(tasks_started: u32, max_tasks: u32) -> bool {
    max_tasks == 0 || tasks_started < max_tasks
}

#[cfg(test)]
mod tests {
    use super::{can_start_more_tasks, effective_active_worker_count, initial_tasks_started};

    use super::super::WorkerRecord;
    use super::super::state;

    #[test]
    fn effective_active_worker_count_uses_max() {
        let workspace_path = crate::testsupport::path::portable_abs_path("ws/RQ-0001");
        let mut state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());
        state_file.upsert_worker(WorkerRecord::new(
            "RQ-0001",
            workspace_path.clone(),
            "2026-02-20T00:00:00Z".to_string(),
        ));

        // Guard in-flight workers are the only capacity authority.
        // State records are persisted history and must not consume capacity.
        assert_eq!(effective_active_worker_count(&state_file, 2), 2);
        assert_eq!(effective_active_worker_count(&state_file, 0), 0);
    }

    #[test]
    fn initial_tasks_started_starts_at_zero_per_invocation() {
        let workspace_path = crate::testsupport::path::portable_abs_path("ws/RQ-0001");
        let mut state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());
        let mut failed = WorkerRecord::new(
            "RQ-0001",
            workspace_path,
            "2026-02-20T00:00:00Z".to_string(),
        );
        failed.mark_failed("2026-02-20T00:05:00Z".to_string(), "failed");
        state_file.upsert_worker(failed);

        assert_eq!(initial_tasks_started(&state_file), 0);
    }

    #[test]
    fn can_start_more_tasks_logic() {
        // max_tasks=0 means unlimited
        assert!(can_start_more_tasks(100, 0));

        // With max_tasks=5
        assert!(can_start_more_tasks(0, 5));
        assert!(can_start_more_tasks(4, 5));
        assert!(!can_start_more_tasks(5, 5));
        assert!(!can_start_more_tasks(6, 5));
    }
}
