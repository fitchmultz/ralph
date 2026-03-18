//! Managed subprocess wait helpers.
//!
//! Purpose:
//! - Own low-level child wait and signal-escalation behavior for managed subprocesses.
//!
//! Responsibilities:
//! - Wait for child exit with timeout and cancellation support.
//! - Apply SIGINT-before-SIGKILL escalation on Unix and platform-appropriate fallback elsewhere.
//! - Return structured termination metadata to the higher-level shell orchestration.
//!
//! Scope:
//! - Child waiting, timeout slicing, and termination signaling only.
//!
//! Usage:
//! - Invoked exclusively by `crate::runutil::shell` during managed subprocess execution.
//!
//! Invariants/assumptions:
//! - Unix children run in isolated process groups before these helpers signal them.
//! - Timeout and cancellation state is reported back without leaf modules reimplementing wait loops.

use std::process::{Child, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::constants::timeouts;

use super::types::TerminationReason;

pub(super) struct ManagedChildOutcome {
    pub status: ExitStatus,
    pub termination: Option<TerminationReason>,
}

#[cfg(unix)]
pub(super) fn wait_for_child_owned(
    child: Child,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
) -> std::io::Result<ManagedChildOutcome> {
    use std::sync::mpsc::{self, Receiver};
    use std::thread;

    fn spawn_exit_waiter(mut child: Child) -> Receiver<std::io::Result<ExitStatus>> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = child.wait();
            let _ = tx.send(result);
        });
        rx
    }

    fn recv_exit_with_timeout(
        rx: &Receiver<std::io::Result<ExitStatus>>,
        timeout: Duration,
    ) -> std::io::Result<Option<ExitStatus>> {
        match rx.recv_timeout(timeout) {
            Ok(result) => result.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(std::io::Error::other(
                "managed subprocess waiter disconnected",
            )),
        }
    }

    let pid = child.id();
    let exit_rx = spawn_exit_waiter(child);
    let start = Instant::now();
    let mut termination: Option<(TerminationReason, Instant)> = None;

    loop {
        if termination.is_none() {
            if cancellation
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst))
            {
                signal_pid(pid, false);
                termination = Some((TerminationReason::Cancelled, Instant::now()));
            } else if start.elapsed() > timeout {
                signal_pid(pid, false);
                termination = Some((TerminationReason::Timeout, Instant::now()));
            }
        }

        if let Some((_, interrupted_at)) = termination
            && interrupted_at.elapsed() > timeouts::MANAGED_SUBPROCESS_INTERRUPT_GRACE
        {
            signal_pid(pid, true);
        }

        match recv_exit_with_timeout(&exit_rx, next_wait_slice(start, timeout, termination))? {
            Some(status) => {
                return Ok(ManagedChildOutcome {
                    status,
                    termination: if status.success() {
                        None
                    } else {
                        termination.map(|(reason, _)| reason)
                    },
                });
            }
            None => continue,
        }
    }
}

#[cfg(not(unix))]
pub(super) fn wait_for_child(
    child: &mut Child,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
) -> std::io::Result<ManagedChildOutcome> {
    wait_for_child_polling(child, timeout, cancellation)
}

#[cfg(not(unix))]
fn wait_for_child_polling(
    child: &mut Child,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
) -> std::io::Result<ManagedChildOutcome> {
    let start = Instant::now();
    let mut termination: Option<(TerminationReason, Instant)> = None;

    loop {
        if termination.is_none() {
            if cancellation
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst))
            {
                signal_child(child, false);
                termination = Some((TerminationReason::Cancelled, Instant::now()));
            } else if start.elapsed() > timeout {
                signal_child(child, false);
                termination = Some((TerminationReason::Timeout, Instant::now()));
            }
        }

        if let Some((_, interrupted_at)) = termination
            && interrupted_at.elapsed() > timeouts::MANAGED_SUBPROCESS_INTERRUPT_GRACE
        {
            signal_child(child, true);
            reap_killed_child(child)?;
        }

        if let Some(status) = child.try_wait()? {
            return Ok(ManagedChildOutcome {
                status,
                termination: if status.success() {
                    None
                } else {
                    termination.map(|(reason, _)| reason)
                },
            });
        }

        std::thread::sleep(timeouts::MANAGED_SUBPROCESS_POLL_INTERVAL);
    }
}

fn next_wait_slice(
    start: Instant,
    timeout: Duration,
    termination: Option<(TerminationReason, Instant)>,
) -> Duration {
    let mut next = timeouts::MANAGED_SUBPROCESS_POLL_INTERVAL;
    let now = Instant::now();
    let timeout_deadline = start + timeout;
    next = next.min(
        timeout_deadline
            .saturating_duration_since(now)
            .max(Duration::from_millis(1)),
    );

    if let Some((_, interrupted_at)) = termination {
        let grace_deadline = interrupted_at + timeouts::MANAGED_SUBPROCESS_INTERRUPT_GRACE;
        next = next.min(
            grace_deadline
                .saturating_duration_since(now)
                .max(Duration::from_millis(1)),
        );
    }

    next.max(Duration::from_millis(1))
}

#[cfg(not(unix))]
fn signal_child(child: &mut Child, _hard_kill: bool) {
    let _ = child.kill();
}

#[cfg(unix)]
fn signal_pid(pid: u32, hard_kill: bool) {
    let signal = if hard_kill {
        libc::SIGKILL
    } else {
        libc::SIGINT
    };
    // SAFETY: signal is sent to the isolated child process group started by execute_managed_command.
    unsafe {
        let _ = libc::kill(-(pid as i32), signal);
    }
}

#[cfg(not(unix))]
fn reap_killed_child(child: &mut Child) -> std::io::Result<()> {
    let _ = child.wait()?;
    Ok(())
}
