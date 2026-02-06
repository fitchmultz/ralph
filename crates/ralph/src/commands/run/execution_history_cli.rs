//! CLI execution history persistence for ETA estimation.
//!
//! Responsibilities:
//! - Persist execution history entries for CLI runs after a task is completed.
//! - Enforce "record only on Done" semantics to avoid polluting ETA samples.
//!
//! Not handled here:
//! - Measuring or accumulating timings (see `execution_timings` and phase instrumentation).
//! - Recording history for TUI runs (handled by `crate::tui::app`).
//! - Parallel worker mode behavior (callers must skip).
//!
//! Invariants/assumptions callers must respect:
//! - Call only after the run completes (best-effort persistence).
//! - Pass the same `phase_count` used for the run.

use crate::commands::run::execution_timings::RunExecutionTimings;
use crate::contracts::{QueueFile, TaskStatus};
use crate::{execution_history, queue};
use anyhow::Result;
use std::path::Path;

/// Attempt to append an execution history entry for a CLI run.
///
/// Returns `Ok(true)` if an entry was recorded, `Ok(false)` if it was skipped
/// (task not Done or timings not representable), and `Err(_)` on I/O/parse errors.
pub(crate) fn try_record_execution_history_for_cli_run(
    repo_root: &Path,
    done_path: &Path,
    task_id: &str,
    phase_count: u8,
    timings: RunExecutionTimings,
) -> Result<bool> {
    let done_file = queue::load_queue_or_default(done_path)?;
    if !task_is_done(&done_file, task_id) {
        return Ok(false);
    }

    let Some(payload) = timings.build_payload(phase_count) else {
        return Ok(false);
    };

    let cache_dir = repo_root.join(".ralph/cache");
    execution_history::record_execution(
        task_id,
        &payload.runner,
        &payload.model,
        phase_count,
        payload.phase_durations,
        payload.total_duration,
        &cache_dir,
    )?;

    Ok(true)
}

fn task_is_done(done_file: &QueueFile, task_id: &str) -> bool {
    done_file
        .tasks
        .iter()
        .any(|t| t.id.trim() == task_id && t.status == TaskStatus::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::run::PhaseType;
    use crate::contracts::{Model, Runner, Task};
    use crate::progress::ExecutionPhase;
    use std::time::Duration;

    fn write_done_file(path: &Path, task_id: &str, status: TaskStatus) -> Result<()> {
        let mut done = QueueFile::default();
        done.tasks.push(Task {
            id: task_id.to_string(),
            status,
            title: "test".to_string(),
            ..Task::default()
        });
        queue::save_queue(path, &done)?;
        Ok(())
    }

    #[test]
    fn records_when_task_is_done_and_timings_present() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path();
        let done_path = repo_root.join(".ralph/done.json");
        std::fs::create_dir_all(repo_root.join(".ralph"))?;
        write_done_file(&done_path, "RQ-0001", TaskStatus::Done)?;

        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(5),
        );

        assert!(try_record_execution_history_for_cli_run(
            repo_root, &done_path, "RQ-0001", 2, timings
        )?);

        let history = execution_history::load_execution_history(&repo_root.join(".ralph/cache"))?;
        assert_eq!(history.entries.len(), 1);
        let entry = &history.entries[0];
        assert_eq!(entry.task_id, "RQ-0001");
        assert_eq!(entry.runner, "codex");
        assert_eq!(entry.model, "gpt-5.3");
        assert_eq!(entry.phase_count, 2);
        assert_eq!(
            entry.phase_durations.get(&ExecutionPhase::Planning),
            Some(&Duration::from_secs(5))
        );
        Ok(())
    }

    #[test]
    fn skips_when_task_not_done() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path();
        let done_path = repo_root.join(".ralph/done.json");
        std::fs::create_dir_all(repo_root.join(".ralph"))?;
        write_done_file(&done_path, "RQ-0001", TaskStatus::Rejected)?;

        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(5),
        );

        assert!(!try_record_execution_history_for_cli_run(
            repo_root, &done_path, "RQ-0001", 2, timings
        )?);
        assert!(
            !repo_root
                .join(".ralph/cache/execution_history.json")
                .exists()
        );
        Ok(())
    }

    #[test]
    fn skips_when_timings_mixed_runner_model() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path();
        let done_path = repo_root.join(".ralph/done.json");
        std::fs::create_dir_all(repo_root.join(".ralph"))?;
        write_done_file(&done_path, "RQ-0001", TaskStatus::Done)?;

        let mut timings = RunExecutionTimings::default();
        timings.record_runner_duration(
            PhaseType::Planning,
            &Runner::Codex,
            &Model::Gpt53,
            Duration::from_secs(5),
        );
        timings.record_runner_duration(
            PhaseType::Implementation,
            &Runner::Claude,
            &Model::Gpt53,
            Duration::from_secs(5),
        );

        assert!(!try_record_execution_history_for_cli_run(
            repo_root, &done_path, "RQ-0001", 2, timings
        )?);
        assert!(
            !repo_root
                .join(".ralph/cache/execution_history.json")
                .exists()
        );
        Ok(())
    }

    #[test]
    fn appends_multiple_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repo_root = temp.path();
        let done_path = repo_root.join(".ralph/done.json");
        std::fs::create_dir_all(repo_root.join(".ralph"))?;
        write_done_file(&done_path, "RQ-0001", TaskStatus::Done)?;

        for _ in 0..2 {
            let mut timings = RunExecutionTimings::default();
            timings.record_runner_duration(
                PhaseType::Planning,
                &Runner::Codex,
                &Model::Gpt53,
                Duration::from_secs(1),
            );
            assert!(try_record_execution_history_for_cli_run(
                repo_root, &done_path, "RQ-0001", 2, timings
            )?);
        }

        let history = execution_history::load_execution_history(&repo_root.join(".ralph/cache"))?;
        assert_eq!(history.entries.len(), 2);
        Ok(())
    }
}
