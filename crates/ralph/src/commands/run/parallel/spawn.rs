//! Register a workspace with the cleanup guard and spawn a worker child process.
//!
//! Purpose:
//! - Register a workspace with the cleanup guard and spawn a worker child process.
//!
//! Responsibilities:
//! - Tie workspace creation, sync, and process spawn into one transactional step for the guard.
//!
//! Not handled here:
//! - Selecting tasks or building runner command lines (see `worker.rs`).
//! - Integration loop or push retries.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - On any error after workspace registration, the guard remains responsible for teardown.

use crate::git;
use anyhow::Result;
use std::path::Path;

use super::cleanup_guard::ParallelCleanupGuard;

pub(crate) fn spawn_worker_with_registered_workspace<CreateWorkspace, SyncWorkspace, SpawnWorker>(
    guard: &mut ParallelCleanupGuard,
    task_id: &str,
    create_workspace: CreateWorkspace,
    sync_workspace: SyncWorkspace,
    spawn: SpawnWorker,
) -> Result<(git::WorkspaceSpec, std::process::Child)>
where
    CreateWorkspace: FnOnce() -> Result<git::WorkspaceSpec>,
    SyncWorkspace: FnOnce(&Path) -> Result<()>,
    SpawnWorker: FnOnce(&git::WorkspaceSpec) -> Result<std::process::Child>,
{
    let workspace = create_workspace()?;
    guard.register_workspace(task_id.to_string(), workspace.clone());
    sync_workspace(&workspace.path)?;
    let child = spawn(&workspace)?;
    Ok((workspace, child))
}

#[cfg(test)]
mod tests {
    use super::spawn_worker_with_registered_workspace;
    use crate::git;
    use anyhow::Result;
    use std::cell::Cell;
    use tempfile::TempDir;

    use super::super::cleanup_guard::ParallelCleanupGuard;
    use super::super::state;

    fn create_test_cleanup_guard(temp: &TempDir) -> ParallelCleanupGuard {
        let workspace_root = temp.path().join("workspaces");
        std::fs::create_dir_all(&workspace_root).expect("create workspace root");

        let state_path = temp.path().join("state.json");
        let state_file =
            state::ParallelStateFile::new("2026-02-20T00:00:00Z".to_string(), "main".to_string());

        ParallelCleanupGuard::new_simple(state_path, state_file, workspace_root)
    }

    #[test]
    fn spawn_failure_cleans_registered_workspace() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_cleanup_guard(&temp);
        let workspace_root = temp.path().join("workspaces");
        let workspace_path = workspace_root.join("RQ-0001");

        let result = spawn_worker_with_registered_workspace(
            &mut guard,
            "RQ-0001",
            || {
                std::fs::create_dir_all(&workspace_path)?;
                Ok(git::WorkspaceSpec {
                    path: workspace_path.clone(),
                    branch: "main".to_string(),
                })
            },
            |_| Ok(()),
            |_| Err(anyhow::anyhow!("spawn failed")),
        );

        assert!(result.is_err());
        guard.cleanup()?;
        assert!(!workspace_path.exists());
        Ok(())
    }

    #[test]
    fn sync_failure_cleans_registered_workspace_without_spawning() -> Result<()> {
        let temp = TempDir::new()?;
        let mut guard = create_test_cleanup_guard(&temp);
        let workspace_root = temp.path().join("workspaces");
        let workspace_path = workspace_root.join("RQ-0002");
        let spawn_called = Cell::new(false);

        let result = spawn_worker_with_registered_workspace(
            &mut guard,
            "RQ-0002",
            || {
                std::fs::create_dir_all(&workspace_path)?;
                Ok(git::WorkspaceSpec {
                    path: workspace_path.clone(),
                    branch: "main".to_string(),
                })
            },
            |_| Err(anyhow::anyhow!("sync failed")),
            |_| {
                spawn_called.set(true);
                Err(anyhow::anyhow!("spawn should not run"))
            },
        );

        assert!(result.is_err());
        assert!(!spawn_called.get());
        guard.cleanup()?;
        assert!(!workspace_path.exists());
        Ok(())
    }
}
