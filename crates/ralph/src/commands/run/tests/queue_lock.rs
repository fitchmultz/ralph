//! Queue lock handling tests for run command.

use super::{find_definitely_dead_pid, resolved_with_repo_root, task_with_status};
use crate::commands::run::run_session::create_session_for_task;
use crate::contracts::{QueueFile, TaskStatus};
use crate::queue;
use crate::testsupport::INTERRUPT_TEST_MUTEX;
use crate::testsupport::reset_ctrlc_interrupt_flag;
use std::sync::Mutex;

#[test]
fn run_one_with_id_locked_skips_reacquiring_queue_lock() -> anyhow::Result<()> {
    // Synchronize with tests that modify the interrupt flag.
    // Hold the mutex for the entire test to prevent any race conditions.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();
    reset_ctrlc_interrupt_flag();

    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let resolved = resolved_with_repo_root(repo_root.clone());

    std::fs::create_dir_all(repo_root.join(".ralph"))?;
    let task = crate::contracts::Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        title: "Test task".to_string(),
        description: None,
        priority: Default::default(),
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph".to_string()],
        evidence: vec!["observed".to_string()],
        plan: vec!["do thing".to_string()],
        notes: vec![],
        request: Some("test request".to_string()),
        agent: None,
        created_at: Some("2026-01-18T00:00:00Z".to_string()),
        updated_at: Some("2026-01-18T00:00:00Z".to_string()),
        completed_at: Some("2026-01-18T01:00:00Z".to_string()),
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        estimated_minutes: None,
        actual_minutes: None,
        parent_id: None,
    };
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task],
        },
    )?;

    let _lock = queue::acquire_queue_lock(&resolved.repo_root, "test lock", false)?;

    let err = crate::commands::run::run_one_with_id_locked(
        &resolved,
        &crate::commands::run::AgentOverrides::default(),
        false,
        "RQ-0001",
        None,
        None,
        None,
    )
    .expect_err("expected runnable status error");

    let query_err = err
        .downcast_ref::<crate::queue::operations::QueueQueryError>()
        .unwrap_or_else(|| panic!("expected QueueQueryError, got: {err:#}"));

    assert!(
        matches!(
            query_err,
            crate::queue::operations::QueueQueryError::TargetTaskNotRunnable {
                status: TaskStatus::Done,
                ..
            }
        ),
        "expected TargetTaskNotRunnable with Done status"
    );

    assert!(
        !crate::commands::run::queue_lock::is_queue_lock_already_held_error(&err),
        "expected not to fail due to queue-lock contention"
    );

    Ok(())
}

#[test]
fn clear_stale_queue_lock_for_resume_removes_stale_lock() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph"))?;

    let lock_dir = crate::lock::queue_lock_dir(&repo_root);
    std::fs::create_dir_all(&lock_dir)?;
    let owner_path = lock_dir.join("owner");

    let stale_pid = find_definitely_dead_pid();
    std::fs::write(
        &owner_path,
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-02-06T00:56:29Z\ncommand: ralph run loop --max-tasks 0\nlabel: run one\n"
        ),
    )?;

    crate::commands::run::queue_lock::clear_stale_queue_lock_for_resume(&repo_root)?;

    assert!(
        !lock_dir.exists(),
        "expected stale queue lock dir to be cleared during resume"
    );

    Ok(())
}

#[test]
fn clear_stale_queue_lock_for_resume_does_not_remove_live_lock() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph"))?;

    let lock_dir = crate::lock::queue_lock_dir(&repo_root);
    let _held = queue::acquire_queue_lock(&repo_root, "live holder", false)?;

    crate::commands::run::queue_lock::clear_stale_queue_lock_for_resume(&repo_root)?;

    assert!(lock_dir.exists(), "expected live queue lock dir to remain");
    let owner = std::fs::read_to_string(lock_dir.join("owner"))?;
    assert!(
        owner.contains("live holder"),
        "expected lock owner label to be unchanged"
    );

    Ok(())
}

#[test]
fn run_loop_auto_resume_clears_stale_queue_lock_before_task_execution() -> anyhow::Result<()> {
    use std::sync::atomic::Ordering;

    struct InterruptGuard {
        previous: bool,
    }

    impl Drop for InterruptGuard {
        fn drop(&mut self) {
            if let Ok(ctrlc) = crate::runner::ctrlc_state() {
                ctrlc.interrupted.store(self.previous, Ordering::SeqCst);
            }
        }
    }

    // Acquire lock to prevent other tests from running while we have the interrupt flag set.
    // This ensures tests that use the runner don't see the flag as true.
    let interrupt_mutex = INTERRUPT_TEST_MUTEX.get_or_init(|| Mutex::new(()));
    let _interrupt_guard = interrupt_mutex.lock().unwrap();

    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph/cache"))?;

    let resolved = resolved_with_repo_root(repo_root.clone());

    // Valid resumable session: Doing task + session.json present.
    queue::save_queue(
        &resolved.queue_path,
        &QueueFile {
            version: 1,
            tasks: vec![task_with_status(TaskStatus::Doing)],
        },
    )?;
    queue::save_queue(&resolved.done_path, &QueueFile::default())?;

    let session = create_session_for_task(
        "RQ-0001",
        &resolved,
        &crate::commands::run::AgentOverrides::default(),
        1,
        None,
    );
    crate::session::save_session(&repo_root.join(".ralph/cache"), &session)?;

    // Stale queue lock left behind by a dead process.
    let lock_dir = crate::lock::queue_lock_dir(&repo_root);
    std::fs::create_dir_all(&lock_dir)?;
    let stale_pid = find_definitely_dead_pid();
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-02-06T00:56:29Z\ncommand: ralph run loop --max-tasks 0\nlabel: run one\n"
        ),
    )?;

    // Prevent the loop from executing the task; we only care that the resume path
    // cleared the stale lock before attempting `run_one`.
    // NOTE: We set the interrupted flag to prevent the loop from actually running tasks.
    // This is a global state mutation that must be restored even if the test panics.
    // We set the flag as close to the run_loop call as possible to minimize interference
    // with other tests that may be running in parallel.
    let ctrlc =
        crate::runner::ctrlc_state().map_err(|e| anyhow::anyhow!("ctrlc init failed: {e}"))?;
    let guard = InterruptGuard {
        previous: ctrlc.interrupted.load(Ordering::SeqCst),
    };
    // Set interrupted immediately before run_loop to minimize the window where
    // other tests might see the flag as true
    ctrlc.interrupted.store(true, Ordering::SeqCst);

    let result = crate::commands::run::run_loop(
        &resolved,
        crate::commands::run::RunLoopOptions {
            max_tasks: 0,
            agent_overrides: crate::commands::run::AgentOverrides::default(),
            force: false,
            auto_resume: true,
            starting_completed: 0,
            non_interactive: true,
            parallel_workers: None,
            wait_when_blocked: false,
            wait_poll_ms: 1000,
            wait_timeout_seconds: 0,
            notify_when_unblocked: false,
            wait_when_empty: false,
            empty_poll_ms: 30_000,
        },
    );
    drop(guard);

    assert!(result.is_err(), "expected run_loop to abort early");
    assert!(
        !lock_dir.exists(),
        "expected stale queue lock to be cleared during resume"
    );

    Ok(())
}
