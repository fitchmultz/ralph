//! Managed subprocess unit tests.
//!
//! Purpose:
//! - Verify bounded capture, timeout, cancellation, and checked-error behavior for managed subprocesses.
//!
//! Responsibilities:
//! - Exercise representative Unix subprocess success and failure paths.
//! - Guard bounded capture truncation and timeout handling contracts.
//! - Ensure checked-command failures include captured stderr context.
//!
//! Scope:
//! - Unit coverage for `crate::runutil::shell` internals only.
//!
//! Usage:
//! - Compiled via `#[cfg(test)]` from `crate::runutil::shell`.
//!
//! Invariants/assumptions:
//! - Unix shell fixtures (`/bin/sh`) are available for these focused unit tests.
//! - Failures should describe the managed subprocess and include useful captured output.

#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::sync::atomic::AtomicBool;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use super::types::{CaptureLimits, ManagedProcessError, RetryWaitError};
#[cfg(unix)]
use super::{
    ManagedCommand, TimeoutClass, execute_checked_command, execute_managed_command,
    sleep_with_cancellation,
};

#[cfg(unix)]
#[test]
fn managed_command_bounds_captured_output() {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg("printf '1234567890abcdef'");
    let output = execute_managed_command(
        ManagedCommand::new(command, "capture", TimeoutClass::Probe).with_capture_limits(
            CaptureLimits {
                stdout_max_bytes: 4,
                stderr_max_bytes: 4,
            },
        ),
    )
    .expect("managed command should succeed");

    assert_eq!(String::from_utf8_lossy(&output.stdout), "cdef");
    assert!(output.stdout_truncated);
}

#[cfg(unix)]
#[test]
fn sleep_with_cancellation_stops_early() {
    let cancelled = AtomicBool::new(true);
    let result = sleep_with_cancellation(Duration::from_secs(1), Some(&cancelled));
    assert!(matches!(result, Err(RetryWaitError::Cancelled)));
}

#[cfg(unix)]
#[test]
fn managed_command_times_out() {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg("sleep 5");
    let err = execute_managed_command(
        ManagedCommand::new(command, "timeout", TimeoutClass::Probe)
            .with_timeout(Duration::from_millis(100)),
    )
    .expect_err("managed command should time out");

    assert!(matches!(err, ManagedProcessError::TimedOut { .. }));
}

#[cfg(unix)]
#[test]
fn execute_checked_command_returns_stdout_on_success() {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg("printf 'ok'");
    let output = execute_checked_command(ManagedCommand::new(
        command,
        "checked success",
        TimeoutClass::MetadataProbe,
    ))
    .expect("managed command should succeed");

    assert_eq!(output.stdout_lossy(), "ok");
}

#[cfg(unix)]
#[test]
fn execute_checked_command_includes_stderr_on_failure() {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg("printf 'boom' >&2; exit 7");
    let err = execute_checked_command(ManagedCommand::new(
        command,
        "checked failure",
        TimeoutClass::AppLaunch,
    ))
    .expect_err("managed command should fail");

    let text = err.to_string();
    assert!(text.contains("checked failure failed"));
    assert!(text.contains("exit status"));
    assert!(text.contains("boom"));
}
