//! Tests for process execution and timeout handling.
//!
//! These tests verify the timeout race condition fix and state transition logic.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::runner::execution::process::{CtrlCState, wait_for_child};

/// Creates a shell command that simulates a slow-exiting process.
/// The process will sleep for `exit_delay_ms` after receiving SIGINT,
/// then exit with the specified code.
fn slow_exit_command(exit_delay_ms: u64, exit_code: i32) -> Command {
    let script = format!(
        r#"trap 'sleep 0.{exit_delay_ms}; exit {exit_code}' INT; while true; do sleep 1; done"#
    );
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(script)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// Creates a CtrlCState for testing.
fn test_ctrlc_state() -> Arc<CtrlCState> {
    Arc::new(CtrlCState {
        active_pgid: Mutex::new(None),
        interrupted: AtomicBool::new(false),
    })
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

    let mut cmd = slow_exit_command(1, 0); // 100ms delay, exit 0

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

    // Set a very short timeout (50ms) to trigger the interrupt quickly
    let timeout = Some(Duration::from_millis(50));

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
    // 1. Ignores SIGINT (trap '' INT)
    // 2. Continues running
    // 3. Will be killed by SIGKILL after grace period

    let script = r#"trap '' INT; while true; do sleep 1; done"#;
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(script)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

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

    let mut cmd = slow_exit_command(1, 42); // 100ms delay, exit 42

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

    let mut cmd = slow_exit_command(1, 0); // 100ms delay, exit 0

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

    // Set the interrupted flag (simulating Ctrl-C)
    // Do this in a separate thread with a small delay to let wait_for_child start
    let ctrlc_clone = Arc::clone(&ctrlc);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        ctrlc_clone.interrupted.store(true, Ordering::SeqCst);
    });

    // No timeout - rely on Ctrl-C
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
