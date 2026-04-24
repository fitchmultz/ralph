//! Wait-state management for runner subprocesses.
//!
//! Purpose:
//! - Wait-state management for runner subprocesses.
//!
//! Responsibilities:
//! - Adapt the shared child wait state machine to runner subprocess semantics.
//! - Route soft and hard interrupts through the active runner process group when available.
//! - Preserve runner-specific timeout and Ctrl-C error mapping for callers.
//!
//! Non-scope:
//! - Spawning the subprocess or wiring stdout/stderr readers.
//! - Output buffer management.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Unix child processes run in isolated process groups.
//! - Timeout-triggered termination still takes precedence over Ctrl-C-triggered termination.

use std::process::{Child, ExitStatus};
use std::time::Duration;

use crate::constants::timeouts;
use crate::runner::RunnerError;
use crate::runutil::{ChildTerminationReason, ChildWaitOptions, wait_for_child_with_callbacks};

use super::CtrlCState;

pub(crate) fn wait_for_child(
    child: Child,
    ctrlc: &CtrlCState,
    timeout: Option<Duration>,
) -> Result<ExitStatus, RunnerError> {
    let outcome = wait_for_child_with_callbacks(
        child,
        ChildWaitOptions {
            timeout,
            cancellation: Some(&ctrlc.interrupted),
            poll_interval: timeouts::MANAGED_SUBPROCESS_POLL_INTERVAL,
            interrupt_grace: timeouts::MANAGED_SUBPROCESS_INTERRUPT_GRACE,
        },
        |child| signal_runner_child(child, ctrlc, false),
        |child| signal_runner_child(child, ctrlc, true),
    )
    .map_err(RunnerError::Io)?;

    finish_wait(outcome.status, outcome.termination)
}

fn finish_wait(
    status: ExitStatus,
    termination: Option<ChildTerminationReason>,
) -> Result<ExitStatus, RunnerError> {
    if status.success() {
        return Ok(status);
    }

    match termination {
        Some(ChildTerminationReason::Timeout) => Err(RunnerError::Timeout),
        Some(ChildTerminationReason::Cancelled) | None => Ok(status),
    }
}

#[cfg(unix)]
fn signal_runner_child(_child: &mut Child, ctrlc: &CtrlCState, hard_kill: bool) {
    let signal = if hard_kill {
        libc::SIGKILL
    } else {
        libc::SIGINT
    };
    signal_process_group(ctrlc, signal);
}

#[cfg(unix)]
fn signal_process_group(ctrlc: &CtrlCState, signal: i32) {
    let pgid = ctrlc
        .active_pgid
        .lock()
        .inspect_err(|e| log::debug!("wait_for_child: failed to lock pgid: {}", e))
        .ok()
        .and_then(|guard| *guard);
    if let Some(pgid) = pgid {
        // SAFETY: the stored pgid belongs to the current child process group.
        unsafe {
            libc::kill(-pgid, signal);
        }
    }
}

#[cfg(not(unix))]
fn signal_runner_child(child: &mut Child, _ctrlc: &CtrlCState, hard_kill: bool) {
    let action = if hard_kill {
        "final kill request"
    } else {
        "kill request"
    };
    if let Err(error) = child.kill() {
        log::debug!("Failed to send {action} to runner child process: {error}");
    }
}
