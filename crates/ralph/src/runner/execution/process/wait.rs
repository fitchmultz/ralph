//! Wait-state management for runner subprocesses.
//!
//! Responsibilities:
//! - Track timeout and interrupt state transitions while a runner subprocess is alive.
//! - Prefer event-driven exit waiting on unix while preserving timeout/interrupt semantics.
//! - Escalate from soft interrupt to hard kill after the configured grace period.
//!
//! Does not handle:
//! - Spawning the subprocess or wiring stdout/stderr readers.
//! - Output buffer management.
//!
//! Assumptions/invariants:
//! - Unix child processes run in isolated process groups.
//! - Timeout errors take precedence over Ctrl-C errors.

use std::process::{Child, ExitStatus};
use std::sync::atomic::Ordering;
#[cfg(unix)]
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use crate::runner::RunnerError;

use super::CtrlCState;

const INTERRUPT_GRACE: Duration = Duration::from_secs(2);
const CTRL_C_CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessState {
    Running,
    TimeoutInterrupt(Instant),
    CtrlCInterrupt(Instant),
    Killed,
}

pub(crate) fn wait_for_child(
    child: &mut Child,
    ctrlc: &CtrlCState,
    timeout: Option<Duration>,
) -> Result<ExitStatus, RunnerError> {
    #[cfg(unix)]
    {
        wait_for_child_unix(child, ctrlc, timeout)
    }
    #[cfg(not(unix))]
    {
        wait_for_child_polling(child, ctrlc, timeout)
    }
}

#[cfg(unix)]
fn wait_for_child_unix(
    child: &mut Child,
    ctrlc: &CtrlCState,
    timeout: Option<Duration>,
) -> Result<ExitStatus, RunnerError> {
    let start = Instant::now();
    let mut state = ProcessState::Running;
    let exit_rx = spawn_exit_waiter(child.id() as i32);

    loop {
        let now = Instant::now();

        if let Some(limit) = timeout
            && now.duration_since(start) > limit
            && state == ProcessState::Running
        {
            log::warn!("Runner timed out after {:?}; sending interrupt", limit);
            state = ProcessState::TimeoutInterrupt(now);
            signal_process_group(ctrlc, libc::SIGINT);
        }

        if ctrlc.interrupted.load(Ordering::SeqCst) && state == ProcessState::Running {
            log::debug!("Ctrl-C detected; sending interrupt to runner process");
            state = ProcessState::CtrlCInterrupt(now);
            signal_process_group(ctrlc, libc::SIGINT);
        }

        if let Some(interrupted_at) = interruption_started_at(state)
            && now.duration_since(interrupted_at) > INTERRUPT_GRACE
            && state != ProcessState::Killed
        {
            log::debug!("Interrupt grace period expired; sending SIGKILL to runner process");
            state = ProcessState::Killed;
            signal_process_group(ctrlc, libc::SIGKILL);
        }

        match recv_exit_with_deadline(&exit_rx, next_wait_slice(start, timeout, state, now)) {
            Ok(status) => return finish_wait(state, status),
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(err) => return Err(RunnerError::Io(err)),
        }
    }
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

#[cfg(unix)]
fn spawn_exit_waiter(pid: i32) -> Receiver<std::io::Result<ExitStatus>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut raw_status = 0;
        // SAFETY: waiting on a known child pid is a standard POSIX operation.
        let result = match unsafe { libc::waitpid(pid, &mut raw_status, 0) } {
            waited if waited == pid => Ok(ExitStatus::from_raw(raw_status)),
            -1 => Err(std::io::Error::last_os_error()),
            waited => Err(std::io::Error::other(format!(
                "waitpid returned unexpected pid {} for {}",
                waited, pid
            ))),
        };
        let _ = tx.send(result);
    });
    rx
}

#[cfg(unix)]
fn recv_exit_with_deadline(
    rx: &Receiver<std::io::Result<ExitStatus>>,
    wait_for: Duration,
) -> std::io::Result<ExitStatus> {
    match rx.recv_timeout(wait_for) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(std::io::Error::from(std::io::ErrorKind::TimedOut))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(std::io::Error::other("runner exit waiter disconnected"))
        }
    }
}

fn next_wait_slice(
    start: Instant,
    timeout: Option<Duration>,
    state: ProcessState,
    now: Instant,
) -> Duration {
    let mut next = CTRL_C_CHECK_INTERVAL;

    if let Some(limit) = timeout
        && state == ProcessState::Running
    {
        let deadline = start + limit;
        next = next.min(
            deadline
                .saturating_duration_since(now)
                .max(Duration::from_millis(1)),
        );
    }

    if let Some(interrupted_at) = interruption_started_at(state) {
        let deadline = interrupted_at + INTERRUPT_GRACE;
        next = next.min(
            deadline
                .saturating_duration_since(now)
                .max(Duration::from_millis(1)),
        );
    }

    next.max(Duration::from_millis(1))
}

fn interruption_started_at(state: ProcessState) -> Option<Instant> {
    match state {
        ProcessState::TimeoutInterrupt(at) | ProcessState::CtrlCInterrupt(at) => Some(at),
        ProcessState::Running | ProcessState::Killed => None,
    }
}

fn finish_wait(state: ProcessState, status: ExitStatus) -> Result<ExitStatus, RunnerError> {
    if status.success() {
        return Ok(status);
    }

    match state {
        ProcessState::TimeoutInterrupt(_) | ProcessState::Killed => Err(RunnerError::Timeout),
        ProcessState::CtrlCInterrupt(_) | ProcessState::Running => Ok(status),
    }
}

#[cfg(not(unix))]
fn wait_for_child_polling(
    child: &mut Child,
    ctrlc: &CtrlCState,
    timeout: Option<Duration>,
) -> Result<ExitStatus, RunnerError> {
    let mut state = ProcessState::Running;
    let start = Instant::now();
    loop {
        let now = Instant::now();
        if let Some(limit) = timeout
            && now.duration_since(start) > limit
            && state == ProcessState::Running
        {
            state = ProcessState::TimeoutInterrupt(now);
            if let Err(e) = child.kill() {
                log::debug!("Failed to send kill request to child process: {}", e);
            }
        }

        if ctrlc.interrupted.load(Ordering::SeqCst) && state == ProcessState::Running {
            state = ProcessState::CtrlCInterrupt(now);
            if let Err(e) = child.kill() {
                log::debug!(
                    "Failed to send kill request to child process on Ctrl-C: {}",
                    e
                );
            }
        }

        if let Some(interrupted_at) = interruption_started_at(state)
            && now.duration_since(interrupted_at) > INTERRUPT_GRACE
        {
            state = ProcessState::Killed;
            if let Err(e) = child.kill() {
                log::debug!("Failed to send final kill request to child process: {}", e);
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => return finish_wait(state, status),
            Ok(None) => {}
            Err(e) => return Err(RunnerError::Io(e)),
        }

        std::thread::sleep(CTRL_C_CHECK_INTERVAL);
    }
}
