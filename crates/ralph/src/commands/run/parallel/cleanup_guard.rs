//! Cleanup guard for parallel run loop to ensure resources are cleaned up on any exit path.
//!
//! Responsibilities:
//! - Own and manage resources that need cleanup: merge runner thread, in-flight workers,
//!   workspace directories, and parallel state.
//! - Perform best-effort cleanup on Drop to prevent resource leaks on early returns.
//!
//! Not handled here:
//! - Actual worker execution logic (see `super::worker`).
//! - Merge runner implementation (see `super::merge_runner`).
//! - State persistence format (see `super::state`).
//!
//! Invariants/assumptions:
//! - Cleanup is idempotent and best-effort; errors are logged but not propagated.
//! - Drop must never panic (no RefCell to avoid panic during unwinding).
//! - The guard owns resources and releases them during cleanup.

use crate::commands::run::parallel::merge_runner::MergeWorkItem;
use crate::commands::run::parallel::state;
use crate::commands::run::parallel::worker::{WorkerState, terminate_workers};
use crate::git::{self, WorkspaceSpec};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

/// Guard that ensures cleanup of parallel run resources on any exit path.
///
/// This guard owns all resources that must be cleaned up, so early returns
/// via `?` still trigger cleanup through the Drop implementation.
///
/// Uses &mut self methods (no RefCell) to guarantee Drop never panics.
pub(crate) struct ParallelCleanupGuard {
    /// Signal to stop the merge runner thread.
    merge_stop: Arc<AtomicBool>,
    /// Sender for PR work items to the merge runner (dropped to unblock receiver).
    pr_tx: Option<mpsc::Sender<MergeWorkItem>>,
    /// Handle to the merge runner thread (joined during cleanup).
    merge_handle: Option<thread::JoinHandle<anyhow::Result<()>>>,
    /// Path to the parallel state file.
    state_path: PathBuf,
    /// In-memory parallel state (persisted during cleanup).
    state_file: state::ParallelStateFile,
    /// Map of in-flight worker processes (terminated during cleanup).
    in_flight: HashMap<String, WorkerState>,
    /// Map of all known workspaces (removed during cleanup).
    workspaces: HashMap<String, WorkspaceSpec>,
    /// Root directory for workspaces.
    workspace_root: PathBuf,
    /// Whether cleanup has already been performed.
    completed: bool,
}

impl ParallelCleanupGuard {
    /// Create a new cleanup guard with the given resources.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        merge_stop: Arc<AtomicBool>,
        pr_tx: mpsc::Sender<MergeWorkItem>,
        merge_handle: Option<thread::JoinHandle<anyhow::Result<()>>>,
        state_path: PathBuf,
        state_file: state::ParallelStateFile,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            merge_stop,
            pr_tx: Some(pr_tx),
            merge_handle,
            state_path,
            state_file,
            in_flight: HashMap::new(),
            workspaces: HashMap::new(),
            workspace_root,
            completed: false,
        }
    }

    /// Mark cleanup as completed successfully (disarms the guard).
    ///
    /// After calling this, Drop will be a no-op.
    pub fn mark_completed(&mut self) {
        self.completed = true;
    }

    /// Get a clone of the PR sender if available.
    pub fn pr_tx(&self) -> Option<mpsc::Sender<MergeWorkItem>> {
        self.pr_tx.clone()
    }

    /// Take ownership of the PR sender (for explicit dropping).
    pub fn take_pr_tx(&mut self) -> Option<mpsc::Sender<MergeWorkItem>> {
        self.pr_tx.take()
    }

    /// Take ownership of the merge handle.
    pub fn take_merge_handle(&mut self) -> Option<thread::JoinHandle<anyhow::Result<()>>> {
        self.merge_handle.take()
    }

    /// Get mutable access to the state file.
    pub fn state_file_mut(&mut self) -> &mut state::ParallelStateFile {
        &mut self.state_file
    }

    /// Get immutable access to the state file.
    pub fn state_file(&self) -> &state::ParallelStateFile {
        &self.state_file
    }

    /// Get immutable access to in-flight workers.
    pub fn in_flight(&self) -> &HashMap<String, WorkerState> {
        &self.in_flight
    }

    /// Poll all workers and return IDs of finished workers along with their exit status.
    ///
    /// This method takes &mut self to allow calling try_wait on child processes.
    pub fn poll_workers(
        &mut self,
    ) -> Vec<(String, String, WorkspaceSpec, std::process::ExitStatus)> {
        let mut finished = Vec::new();
        let task_ids: Vec<String> = self.in_flight.keys().cloned().collect();

        for task_id in task_ids {
            if let Some(worker) = self.in_flight.get_mut(&task_id)
                && let Ok(Some(status)) = worker.child.try_wait()
            {
                finished.push((
                    task_id,
                    worker.task_title.clone(),
                    worker.workspace.clone(),
                    status,
                ));
            }
        }

        finished
    }

    /// Register a workspace for cleanup.
    pub fn register_workspace(&mut self, task_id: String, spec: WorkspaceSpec) {
        self.workspaces.insert(task_id, spec);
    }

    /// Register an in-flight worker for cleanup.
    pub fn register_worker(&mut self, task_id: String, worker: WorkerState) {
        self.in_flight.insert(task_id, worker);
    }

    /// Remove a worker from cleanup tracking (e.g., after completion).
    pub fn remove_worker(&mut self, task_id: &str) -> Option<WorkerState> {
        self.in_flight.remove(task_id)
    }

    /// Perform full cleanup and return any error.
    ///
    /// This is idempotent - safe to call multiple times.
    pub fn cleanup(&mut self) -> Result<()> {
        if self.completed {
            return Ok(());
        }

        log::debug!("ParallelCleanupGuard: performing cleanup");

        // Step 1: Signal merge runner to stop
        self.merge_stop.store(true, Ordering::SeqCst);

        // Step 2: Drop the PR sender to unblock the merge runner's receiver
        drop(self.pr_tx.take());

        // Step 3: Join the merge runner thread
        if let Some(handle) = self.merge_handle.take() {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    log::warn!("Merge runner thread returned error: {:#}", err);
                }
                Err(err) => {
                    log::warn!("Merge runner thread panicked: {:?}", err);
                }
            }
        }

        // Step 4: Terminate in-flight workers
        terminate_workers(&mut self.in_flight);

        // Step 5: Remove all tracked workspaces
        for (task_id, spec) in &self.workspaces {
            if spec.path.exists()
                && let Err(err) = git::remove_workspace(&self.workspace_root, spec, true)
            {
                log::warn!(
                    "Failed to remove workspace for {} during cleanup: {:#}",
                    task_id,
                    err
                );
            }
        }

        // Step 6: Clear tasks_in_flight and persist state
        self.state_file.tasks_in_flight.clear();
        if let Err(err) = state::save_state(&self.state_path, &self.state_file) {
            log::warn!("Failed to save parallel state during cleanup: {:#}", err);
        }

        self.completed = true;
        Ok(())
    }

    /// Perform cleanup, logging and suppressing any errors.
    ///
    /// This is called from Drop to ensure cleanup never panics.
    fn cleanup_best_effort(&mut self) {
        if let Err(err) = self.cleanup() {
            log::warn!("ParallelCleanupGuard: cleanup error: {:#}", err);
        }
    }
}

impl Drop for ParallelCleanupGuard {
    fn drop(&mut self) {
        // Ensure cleanup runs even if the guard is dropped without explicit cleanup call
        self.cleanup_best_effort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ParallelMergeMethod, ParallelMergeWhen};
    use crate::lock;
    use std::process::{Child, Command};
    use tempfile::TempDir;

    fn create_test_guard(temp: &TempDir) -> ParallelCleanupGuard {
        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).unwrap();

        let state_path = temp.path().join("state.json");
        let state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        let (pr_tx, _pr_rx) = mpsc::channel::<MergeWorkItem>();
        let merge_stop = Arc::new(AtomicBool::new(false));

        ParallelCleanupGuard::new(
            merge_stop,
            pr_tx,
            None,
            state_path,
            state_file,
            workspace_root,
        )
    }

    #[test]
    fn guard_cleanup_kills_worker_and_clears_state() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_guard(&temp);

        // Spawn a long-lived child process
        let child: Child = Command::new("sleep").arg("10").spawn()?;
        let pid = child.id();

        // Create a workspace for the worker
        let workspace_path = temp.path().join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        // Create worker state
        let worker = WorkerState {
            task_id: "RQ-0001".to_string(),
            task_title: "Test task".to_string(),
            workspace: WorkspaceSpec {
                path: workspace_path.clone(),
                branch: "ralph/RQ-0001".to_string(),
            },
            child,
        };

        // Register worker and add to state
        guard.register_worker("RQ-0001".to_string(), worker);
        guard
            .state_file_mut()
            .upsert_task(state::ParallelTaskRecord {
                task_id: "RQ-0001".to_string(),
                workspace_path: workspace_path.to_string_lossy().to_string(),
                branch: "ralph/RQ-0001".to_string(),
                pid: Some(pid),
                started_at: "2026-02-02T00:00:00Z".to_string(),
            });

        // Verify worker is running
        assert_eq!(
            lock::pid_is_running(pid),
            Some(true),
            "Worker should be running before cleanup"
        );

        // Perform cleanup
        guard.cleanup()?;

        // Verify worker is terminated (allow for indeterminate result)
        let running = lock::pid_is_running(pid);
        assert!(
            running == Some(false) || running.is_none(),
            "Worker should be terminated after cleanup, got: {:?}",
            running
        );

        // Verify state is cleared
        assert!(
            guard.state_file.tasks_in_flight.is_empty(),
            "tasks_in_flight should be empty after cleanup"
        );

        Ok(())
    }

    #[test]
    fn guard_cleanup_stops_and_joins_merge_runner() -> Result<()> {
        let temp = TempDir::new()?;

        let state_path = temp.path().join("state.json");
        let state_file = state::ParallelStateFile::new(
            "2026-02-01T00:00:00Z".to_string(),
            "main".to_string(),
            ParallelMergeMethod::Squash,
            ParallelMergeWhen::AsCreated,
        );

        let (pr_tx, pr_rx) = mpsc::channel::<MergeWorkItem>();
        let merge_stop = Arc::new(AtomicBool::new(false));
        let thread_exited = Arc::new(AtomicBool::new(false));
        let thread_exited_clone = Arc::clone(&thread_exited);
        let merge_stop_clone = Arc::clone(&merge_stop);

        // Spawn a dummy merge runner thread
        let handle = thread::spawn(move || {
            loop {
                if merge_stop_clone.load(Ordering::SeqCst) {
                    break;
                }
                match pr_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(_) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            thread_exited_clone.store(true, Ordering::SeqCst);
            Ok(())
        });

        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root)?;

        let mut guard = ParallelCleanupGuard::new(
            merge_stop,
            pr_tx,
            Some(handle),
            state_path,
            state_file,
            workspace_root,
        );

        // Verify thread hasn't exited yet
        assert!(
            !thread_exited.load(Ordering::SeqCst),
            "Thread should not have exited before cleanup"
        );

        // Perform cleanup
        guard.cleanup()?;

        // Verify thread has exited
        assert!(
            thread_exited.load(Ordering::SeqCst),
            "Thread should have exited after cleanup"
        );

        Ok(())
    }

    #[test]
    fn guard_disarm_prevents_cleanup() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_guard(&temp);

        // Spawn a child process
        let child: Child = Command::new("sleep").arg("10").spawn()?;
        let pid = child.id();

        // Create workspace
        let workspace_path = temp.path().join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        let worker = WorkerState {
            task_id: "RQ-0001".to_string(),
            task_title: "Test task".to_string(),
            workspace: WorkspaceSpec {
                path: workspace_path.clone(),
                branch: "ralph/RQ-0001".to_string(),
            },
            child,
        };

        guard.register_worker("RQ-0001".to_string(), worker);

        // Disarm the guard
        guard.mark_completed();

        // Drop the guard (should not cleanup because it's disarmed)
        drop(guard);

        // Verify worker is still running
        assert_eq!(
            lock::pid_is_running(pid),
            Some(true),
            "Worker should still be running after disarmed drop"
        );

        // Clean up the child process
        let _ = Command::new("kill").arg(pid.to_string()).output();

        Ok(())
    }

    #[test]
    fn guard_cleanup_is_idempotent() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_guard(&temp);

        // Spawn a child process
        let child: Child = Command::new("sleep").arg("10").spawn()?;
        let pid = child.id();

        // Create workspace
        let workspace_path = temp.path().join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        let worker = WorkerState {
            task_id: "RQ-0001".to_string(),
            task_title: "Test task".to_string(),
            workspace: WorkspaceSpec {
                path: workspace_path.clone(),
                branch: "ralph/RQ-0001".to_string(),
            },
            child,
        };

        guard.register_worker("RQ-0001".to_string(), worker);

        // First cleanup
        guard.cleanup()?;

        // Verify worker is terminated (allow for indeterminate result)
        let running = lock::pid_is_running(pid);
        assert!(
            running == Some(false) || running.is_none(),
            "Worker should be terminated after first cleanup, got: {:?}",
            running
        );

        // Second cleanup should be a no-op (idempotent)
        guard.cleanup()?;

        Ok(())
    }

    #[test]
    fn guard_cleanup_runs_on_drop() -> Result<()> {
        let temp = TempDir::new()?;

        let mut guard = create_test_guard(&temp);

        let child: Child = Command::new("sleep").arg("10").spawn()?;
        let pid: u32 = child.id();

        let workspace_path = temp.path().join("workspaces").join("RQ-0001");
        std::fs::create_dir_all(&workspace_path)?;

        let worker = WorkerState {
            task_id: "RQ-0001".to_string(),
            task_title: "Test task".to_string(),
            workspace: WorkspaceSpec {
                path: workspace_path,
                branch: "ralph/RQ-0001".to_string(),
            },
            child,
        };

        guard.register_worker("RQ-0001".to_string(), worker);

        // Verify worker is running
        assert_eq!(
            lock::pid_is_running(pid),
            Some(true),
            "Worker should be running before drop"
        );

        // Explicitly drop the guard to trigger cleanup
        // This ensures temp dir is still valid during cleanup
        drop(guard);

        // Verify worker is terminated after guard is dropped
        // Allow for indeterminate result (None) as the process may have
        // been reaped by the time we check
        let running = lock::pid_is_running(pid);
        assert!(
            running == Some(false) || running.is_none(),
            "Worker should be terminated after guard drop, got: {:?}",
            running
        );

        Ok(())
    }
}
