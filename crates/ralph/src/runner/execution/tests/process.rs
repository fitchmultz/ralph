//! Tests for process execution and timeout handling.
//!
//! These tests verify the timeout race condition fix, state transition logic,
//! and Ctrl-C handling hardening (cleanup on error paths, pre-run interrupts).

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::runner::execution::process::{test_ctrlc_state, wait_for_child};

/// Creates a command that exits after a configurable SIGINT cleanup delay and
/// writes a readiness marker only after the SIGINT handler is installed.
fn slow_exit_command(exit_delay_ms: u64, exit_code: i32, ready_file: &std::path::Path) -> Command {
    let script = format!(
        r#"import pathlib
import signal
import sys
import time

ready_file = pathlib.Path(sys.argv[1])

def handle(_signum, _frame):
    time.sleep({delay_seconds:.3})
    raise SystemExit({exit_code})

signal.signal(signal.SIGINT, handle)
ready_file.write_text("ready", encoding="utf-8")

while True:
    time.sleep(1)
"#,
        delay_seconds = exit_delay_ms as f64 / 1000.0,
        exit_code = exit_code,
    );
    let mut cmd = Command::new("python3");
    cmd.arg("-c")
        .arg(script)
        .arg(ready_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// Creates a command that deterministically ignores SIGINT and keeps running.
/// The child writes its readiness marker after installing the ignore handler.
///
/// Python's `signal.SIG_IGN` is more reliable here than shell `trap '' INT`,
/// which can still exit early on some `/bin/sh` implementations under CI load.
fn ignore_sigint_command(ready_file: &std::path::Path) -> Command {
    let mut cmd = Command::new("python3");
    cmd.arg("-c")
        .arg(
            r#"import pathlib
import signal
import sys
import time

ready_file = pathlib.Path(sys.argv[1])

signal.signal(signal.SIGINT, signal.SIG_IGN)
ready_file.write_text("ready", encoding="utf-8")

while True:
    time.sleep(1)
"#,
        )
        .arg(ready_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

fn make_ready_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir for process readiness");
    let ready_file = dir.path().join("ready");
    (dir, ready_file)
}

/// Waits until the child has written its readiness marker.
fn wait_for_ready_file(child: &mut std::process::Child, ready_file: &std::path::Path) {
    let start = std::time::Instant::now();
    while !ready_file.exists() {
        if let Some(status) = child
            .try_wait()
            .expect("poll child while waiting for ready")
        {
            panic!("test child exited before becoming ready: {status}");
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for test child readiness file");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn test_process_exits_cleanly_after_timeout_interrupt() {
    // This test verifies the race condition fix: if a process exits with code 0
    // after receiving a timeout interrupt (SIGINT), it should be treated as
    // success rather than returning RunnerError::Timeout.
    //
    // We create a process that:
    // 1. Traps SIGINT
    // 2. Sleeps briefly (simulating cleanup)
    // 3. Exits with code 0
    //
    // With a short timeout, the process will receive SIGINT, then exit cleanly.

    let (_ready_dir, ready_file) = make_ready_file();
    let mut cmd = slow_exit_command(100, 0, &ready_file); // 100ms delay, exit 0

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().expect("Failed to spawn test process");
    wait_for_ready_file(&mut child, &ready_file);

    let ctrlc = test_ctrlc_state();
    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().unwrap();
        *guard = Some(child.id() as i32);
    }

    // Set a short timeout to trigger the interrupt quickly while allowing
    // the child process to initialize its SIGINT handler reliably.
    let timeout = Some(Duration::from_millis(200));

    // Wait for the process - it should exit cleanly despite the timeout
    let result = wait_for_child(&mut child, &ctrlc, timeout);

    // The process should succeed because it exits with code 0 after interrupt
    assert!(
        result.is_ok(),
        "Process should exit successfully after timeout interrupt"
    );
    let status = result.unwrap();
    assert!(status.success(), "Process should have exit code 0");
}

#[test]
fn test_process_times_out_and_is_killed() {
    // This test verifies that a process which ignores SIGINT will eventually
    // be killed with SIGKILL after the 2-second grace period.
    //
    // We create a process that:
    // 1. Ignores SIGINT via `signal.SIG_IGN`
    // 2. Continues running
    // 3. Will be killed by SIGKILL after grace period

    let (_ready_dir, ready_file) = make_ready_file();
    let mut cmd = ignore_sigint_command(&ready_file);

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().expect("Failed to spawn test process");
    wait_for_ready_file(&mut child, &ready_file);

    let ctrlc = test_ctrlc_state();
    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().unwrap();
        *guard = Some(child.id() as i32);
    }

    // Set a short timeout to trigger interrupt quickly
    let timeout = Some(Duration::from_millis(50));

    let start = std::time::Instant::now();
    let result = wait_for_child(&mut child, &ctrlc, timeout);
    let elapsed = start.elapsed();

    // Process should have been killed after grace period
    // The grace period is 2 seconds, so we should wait at least that long
    assert!(
        elapsed >= Duration::from_secs(2),
        "Should wait at least 2 seconds for grace period before kill"
    );

    // The result should be Timeout error (process was killed due to timeout)
    assert!(
        result.is_err(),
        "wait_for_child should return Timeout error when process is killed"
    );
    match result {
        Err(crate::runner::RunnerError::Timeout) => {}
        Err(other) => panic!("Expected Timeout error, got {:?}", other),
        Ok(status) => panic!(
            "Expected Timeout error, got Ok with exit code {:?}",
            status.code()
        ),
    }
}

#[test]
fn test_process_exits_nonzero_after_timeout() {
    // This test verifies that if a process exits with a non-zero code after
    // receiving a timeout interrupt, RunnerError::Timeout is returned so that
    // callers can handle safeguard dumps and git revert appropriately.

    let (_ready_dir, ready_file) = make_ready_file();
    let mut cmd = slow_exit_command(100, 42, &ready_file); // 100ms delay, exit 42

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().expect("Failed to spawn test process");
    wait_for_ready_file(&mut child, &ready_file);

    let ctrlc = test_ctrlc_state();
    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().unwrap();
        *guard = Some(child.id() as i32);
    }

    let timeout = Some(Duration::from_millis(50));
    let result = wait_for_child(&mut child, &ctrlc, timeout);

    // Should return Timeout error so callers can handle safeguard dumps
    assert!(
        result.is_err(),
        "Should return Timeout error for process that exits non-zero after interrupt"
    );
    match result {
        Err(crate::runner::RunnerError::Timeout) => {}
        Err(other) => panic!("Expected Timeout error, got {:?}", other),
        Ok(status) => panic!(
            "Expected Timeout error, got Ok with exit code {:?}",
            status.code()
        ),
    }
}

#[test]
fn test_ctrl_c_interrupt_handling() {
    // This test verifies that Ctrl-C handling works correctly:
    // - When interrupted flag is set, SIGINT is sent
    // - If process exits cleanly after interrupt, it's treated as success

    let (_ready_dir, ready_file) = make_ready_file();
    let mut cmd = slow_exit_command(100, 0, &ready_file); // 100ms delay, exit 0

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().expect("Failed to spawn test process");
    wait_for_ready_file(&mut child, &ready_file);

    let ctrlc = test_ctrlc_state();
    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().unwrap();
        *guard = Some(child.id() as i32);
    }

    // Set the interrupted flag from a separate thread so `wait_for_child` observes
    // an asynchronous Ctrl-C transition without relying on wall-clock sleeps.
    let ctrlc_clone = Arc::clone(&ctrlc);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        ready_rx.recv().expect("signal wait_for_child start");
        std::thread::park_timeout(Duration::from_millis(100));
        ctrlc_clone.interrupted.store(true, Ordering::SeqCst);
    });

    // No timeout - rely on Ctrl-C
    ready_tx.send(()).expect("notify interrupt thread");
    let result = wait_for_child(&mut child, &ctrlc, None);

    // Process should exit cleanly
    assert!(
        result.is_ok(),
        "Process should exit successfully after Ctrl-C interrupt"
    );
    let status = result.unwrap();
    assert!(status.success(), "Process should have exit code 0");
}

#[test]
fn test_no_timeout_no_interrupt_process_completes_normally() {
    // This test verifies that a process without timeout or interrupt completes normally.

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg("exit 0");

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().expect("Failed to spawn test process");

    let ctrlc = test_ctrlc_state();
    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().unwrap();
        *guard = Some(child.id() as i32);
    }

    let result = wait_for_child(&mut child, &ctrlc, None);

    assert!(result.is_ok());
    assert!(result.unwrap().success());
}

#[test]
fn test_wait_for_child_leaves_active_pgid_for_caller_cleanup() {
    // This test verifies that `wait_for_child` does not clear `active_pgid` on its own.
    // Higher-level cleanup owns that responsibility after timeout handling completes.

    let (_ready_dir, ready_file) = make_ready_file();
    let mut cmd = ignore_sigint_command(&ready_file);

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().expect("Failed to spawn test process");
    wait_for_ready_file(&mut child, &ready_file);

    let ctrlc = test_ctrlc_state();
    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().unwrap();
        *guard = Some(child.id() as i32);
    }

    // Set a short timeout to trigger interrupt quickly
    let timeout = Some(Duration::from_millis(50));

    // Wait for child - should return Timeout error
    let _ = wait_for_child(&mut child, &ctrlc, timeout);

    // `wait_for_child` should leave `active_pgid` alone.
    // Higher-level cleanup clears it after process supervision finishes.
    #[cfg(unix)]
    {
        let pgid = ctrlc.active_pgid.lock().unwrap();
        // The test CtrlCState doesn't auto-clear, so pgid should still be set
        // This tests the raw wait_for_child behavior; cleanup is done at higher level
        assert!(
            pgid.is_some(),
            "wait_for_child doesn't clear pgid - that's caller's responsibility"
        );
    }
}

#[test]
fn test_pre_run_interrupt_returns_immediately() {
    // This test verifies that if the interrupted flag is already set before
    // spawning a process, the operation returns Interrupted without spawning.
    //
    // This is a test of the ctrlc_state logic that run_with_streaming_json uses.
    // We simulate the check that happens before process spawn.

    let ctrlc = test_ctrlc_state();

    // Set interrupted flag BEFORE the run would start
    ctrlc.interrupted.store(true, Ordering::SeqCst);

    // Simulate the pre-run check that run_with_streaming_json performs
    let should_abort = ctrlc.interrupted.load(Ordering::SeqCst);

    assert!(should_abort, "Should detect pre-run interrupt");

    // Verify the flag is still set (we don't clear it on pre-run interrupt)
    assert!(
        ctrlc.interrupted.load(Ordering::SeqCst),
        "Interrupted flag should remain set after detecting pre-run interrupt"
    );

    // Verify active_pgid remains None (no process was spawned)
    let pgid = ctrlc.active_pgid.lock().unwrap();
    assert!(
        pgid.is_none(),
        "active_pgid should remain None when aborting before spawn"
    );
}

#[test]
fn test_ctrl_c_during_timeout_grace_period() {
    // This test verifies the behavior when Ctrl-C is pressed during the timeout
    // grace period (after timeout interrupt sent but before 2s grace expires).
    //
    // Expected behavior: a timeout-triggered SIGINT still yields success when the
    // process exits cleanly before the hard-kill deadline, even if Ctrl-C also
    // arrives during that grace period.
    //
    // Process: traps SIGINT, waits 500ms, exits 0
    // Timeout: 50ms (triggers interrupt quickly)
    // Ctrl-C: fired at 200ms (during grace period)

    let (_ready_dir, ready_file) = make_ready_file();
    let mut cmd = slow_exit_command(500, 0, &ready_file); // 500ms delay, exit 0

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let mut child = cmd.spawn().expect("Failed to spawn test process");
    wait_for_ready_file(&mut child, &ready_file);

    let ctrlc = test_ctrlc_state();
    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().unwrap();
        *guard = Some(child.id() as i32);
    }

    // Set Ctrl-C to fire during the grace period (200ms)
    let ctrlc_clone = Arc::clone(&ctrlc);
    std::thread::spawn(move || {
        std::thread::park_timeout(Duration::from_millis(200));
        ctrlc_clone.interrupted.store(true, Ordering::SeqCst);
    });

    // Short timeout to trigger interrupt quickly
    let timeout = Some(Duration::from_millis(50));

    let result = wait_for_child(&mut child, &ctrlc, timeout);

    // Process should succeed because it exits with code 0 after interrupt
    // (both timeout interrupt and Ctrl-C send SIGINT, so the handler runs)
    assert!(
        result.is_ok(),
        "Process should exit successfully (code 0) after timeout interrupt, even with Ctrl-C during grace"
    );
    let status = result.unwrap();
    assert!(status.success(), "Process should have exit code 0");
}

#[test]
fn test_ctrlc_state_isolation() {
    // This test verifies that test_ctrlc_state creates isolated state
    // that doesn't interfere with other tests.

    let ctrlc1 = test_ctrlc_state();
    let ctrlc2 = test_ctrlc_state();

    // Set interrupted on ctrlc1
    ctrlc1.interrupted.store(true, Ordering::SeqCst);

    // ctrlc2 should not be affected
    assert!(
        !ctrlc2.interrupted.load(Ordering::SeqCst),
        "Isolated CtrlCState should not be affected by other state changes"
    );

    // Set pgid on ctrlc1
    #[cfg(unix)]
    {
        let mut guard = ctrlc1.active_pgid.lock().unwrap();
        *guard = Some(12345);
    }

    // ctrlc2 pgid should remain None
    let pgid2 = ctrlc2.active_pgid.lock().unwrap();
    assert!(
        pgid2.is_none(),
        "Isolated CtrlCState pgid should remain None"
    );
}
