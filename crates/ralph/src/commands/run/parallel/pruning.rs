//! Drop stale terminal worker records from persisted parallel state.
//!
//! Purpose:
//! - Drop stale terminal worker records from persisted parallel state.
//!
//! Responsibilities:
//! - Remove terminal workers with invalid timestamps or past TTL so capacity bookkeeping does not stall.
//!
//! Not handled here:
//! - Persisting state after pruning (callers save).
//! - Filesystem cleanup for workspace directories.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Active (non-terminal) workers are never removed here.

use crate::timeutil;

use super::state;

pub(crate) fn prune_stale_workers(state_file: &mut state::ParallelStateFile) -> Vec<String> {
    let now = time::OffsetDateTime::now_utc();
    let ttl_secs: i64 = crate::constants::timeouts::PARALLEL_TERMINAL_WORKER_TTL
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);

    let mut dropped = Vec::new();
    state_file.workers.retain(|worker| {
        // Don't prune active workers
        if !worker.is_terminal() {
            return true;
        }

        // Time-bound terminal workers so they don't block capacity forever
        let Some(started_at) = timeutil::parse_rfc3339_opt(&worker.started_at) else {
            log::warn!(
                "Dropping stale worker {} with invalid started_at (workspace: {}).",
                worker.task_id,
                worker.workspace_path.display()
            );
            dropped.push(worker.task_id.clone());
            return false;
        };

        let age_secs = (now.unix_timestamp() - started_at.unix_timestamp()).max(0);
        if age_secs >= ttl_secs {
            log::warn!(
                "Dropping stale worker {} after TTL (age_secs={}, ttl_secs={}, started_at='{}', workspace: {}).",
                worker.task_id,
                age_secs,
                ttl_secs,
                worker.started_at,
                worker.workspace_path.display()
            );
            dropped.push(worker.task_id.clone());
            return false;
        }

        true
    });
    dropped
}

#[cfg(test)]
mod tests {
    use super::prune_stale_workers;
    use crate::timeutil;
    use anyhow::Result;
    use std::path::PathBuf;
    use tempfile::TempDir;

    use super::super::WorkerRecord;
    use super::super::state;

    #[test]
    fn prune_stale_workers_retains_recent_terminal_with_missing_workspace() -> Result<()> {
        let mut state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        let mut worker = WorkerRecord::new(
            "RQ-0001",
            PathBuf::from("/nonexistent/path/RQ-0001"),
            timeutil::now_utc_rfc3339_or_fallback(),
        );
        worker.mark_completed(timeutil::now_utc_rfc3339_or_fallback());
        state_file.upsert_worker(worker);

        let dropped = prune_stale_workers(&mut state_file);

        assert!(dropped.is_empty());
        assert_eq!(state_file.workers.len(), 1);
        assert_eq!(state_file.workers[0].task_id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn prune_stale_workers_retains_active() -> Result<()> {
        let temp = TempDir::new()?;
        let workspace_path = temp.path().join("RQ-0002");
        std::fs::create_dir_all(&workspace_path)?;

        let mut state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        // Active worker (not terminal)
        let worker = WorkerRecord::new(
            "RQ-0002",
            workspace_path,
            timeutil::now_utc_rfc3339_or_fallback(),
        );
        state_file.upsert_worker(worker);

        let dropped = prune_stale_workers(&mut state_file);

        assert!(dropped.is_empty());
        assert_eq!(state_file.workers.len(), 1);
        Ok(())
    }
}
