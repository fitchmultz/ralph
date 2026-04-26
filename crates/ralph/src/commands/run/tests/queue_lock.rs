//! Queue lock handling tests for run command.
//!
//! Purpose:
//! - Queue lock handling tests for run command.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

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
        crate::commands::run::RunOneResumeOptions::disabled(),
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
fn inspect_queue_lock_reports_stale_lock_as_stale_operator_state() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph"))?;

    let lock_dir = crate::lock::queue_lock_dir(&repo_root);
    std::fs::create_dir_all(&lock_dir)?;
    let stale_pid = find_definitely_dead_pid();
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {stale_pid}\nstarted_at: 2026-02-06T00:56:29Z\ncommand: ralph run loop --parallel 4\nlabel: run loop\n"
        ),
    )?;

    let inspection = crate::commands::run::queue_lock::inspect_queue_lock(&repo_root)
        .expect("expected queue lock inspection");

    assert_eq!(
        inspection.condition,
        crate::commands::run::queue_lock::QueueLockCondition::Stale
    );
    assert!(matches!(
        &inspection.blocking_state.reason,
        crate::contracts::BlockingReason::LockBlocked { .. }
    ));
    assert!(
        inspection
            .blocking_state
            .message
            .contains("stale queue lock"),
        "expected stale-specific message, got: {}",
        inspection.blocking_state.message
    );
    assert!(
        inspection.blocking_state.detail.contains("auto-clear"),
        "expected stale-specific auto-clear detail, got: {}",
        inspection.blocking_state.detail
    );

    Ok(())
}

#[test]
fn inspect_queue_lock_reports_pid_reuse_review_for_aged_live_owner() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    std::fs::create_dir_all(repo_root.join(".ralph"))?;

    let lock_dir = crate::lock::queue_lock_dir(&repo_root);
    std::fs::create_dir_all(&lock_dir)?;
    std::fs::write(
        lock_dir.join("owner"),
        format!(
            "pid: {}\nstarted_at: 2020-01-01T00:00:00Z\ncommand: ralph run loop --parallel 4\nlabel: run loop\n",
            std::process::id()
        ),
    )?;

    let inspection = crate::commands::run::queue_lock::inspect_queue_lock(&repo_root)
        .expect("expected queue lock inspection");

    assert_eq!(
        inspection.condition,
        crate::commands::run::queue_lock::QueueLockCondition::Live
    );
    assert!(
        inspection.blocking_state.detail.contains("reused PID"),
        "expected PID-reuse review detail, got: {}",
        inspection.blocking_state.detail
    );
    assert!(
        inspection
            .blocking_state
            .detail
            .contains("does not auto-clear"),
        "expected conservative cleanup detail, got: {}",
        inspection.blocking_state.detail
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

    // Valid resumable session: Doing task + session.jsonc present.
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
            run_event_handler: None,
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

#[test]
fn run_one_parallel_worker_acquires_queue_lock() -> anyhow::Result<()> {
    use crate::contracts::Task;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    let temp = tempfile::TempDir::new()?;
    let repo_root = temp.path().to_path_buf();
    let ralph_dir = repo_root.join(".ralph");
    std::fs::create_dir_all(&ralph_dir)?;

    let queue_path = ralph_dir.join("queue.json");
    let mut queue_file = QueueFile {
        version: 1,
        tasks: vec![],
    };
    queue_file.tasks.push(Task {
        id: "RQ-0001".to_string(),
        title: "Test task".to_string(),
        description: None,
        status: TaskStatus::Todo,
        priority: crate::contracts::TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: None,
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
    });
    queue::save_queue(&queue_path, &queue_file)?;

    let _test_lock = queue::acquire_queue_lock(&repo_root, "test lock", false)?;

    let repo_root_clone = repo_root.clone();
    let lock_acquired = Arc::new(AtomicBool::new(false));
    let lock_acquired_clone = Arc::clone(&lock_acquired);

    let handle = thread::spawn(move || {
        let result = queue::acquire_queue_lock(&repo_root_clone, "parallel worker", false);

        if let Err(err) = result {
            let err_str = err.to_string();
            if err_str.contains("Queue lock already held") || err_str.contains("already held") {
                lock_acquired_clone.store(false, Ordering::SeqCst);
            }
        } else {
            lock_acquired_clone.store(true, Ordering::SeqCst);
            drop(result);
        }
    });

    handle.join().expect("thread panicked");

    assert!(
        !lock_acquired.load(Ordering::SeqCst),
        "Expected lock contention error when queue lock is already held"
    );

    Ok(())
}
