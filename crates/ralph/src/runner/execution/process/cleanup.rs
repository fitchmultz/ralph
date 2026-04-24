//! Cleanup helpers for runner process execution.
//!
//! Purpose:
//! - Cleanup helpers for runner process execution.
//!
//! Responsibilities:
//! - Clear active process-group tracking on exit.
//! - Join stdout/stderr reader threads so stream buffers are complete before use.
//!
//! Non-scope:
//! - Process waiting or signal escalation.
//! - Runner output parsing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Cleanup is idempotent.
//! - Drop must never panic.

use std::thread;

use super::CtrlCState;

type ReaderResult = anyhow::Result<()>;

pub(super) struct ProcessCleanupGuard<'a> {
    ctrlc: &'a CtrlCState,
    stdout_handle: Option<thread::JoinHandle<ReaderResult>>,
    stderr_handle: Option<thread::JoinHandle<ReaderResult>>,
    completed: bool,
}

impl<'a> ProcessCleanupGuard<'a> {
    pub(super) fn new(
        ctrlc: &'a CtrlCState,
        stdout_handle: thread::JoinHandle<ReaderResult>,
        stderr_handle: thread::JoinHandle<ReaderResult>,
    ) -> Self {
        Self {
            ctrlc,
            stdout_handle: Some(stdout_handle),
            stderr_handle: Some(stderr_handle),
            completed: false,
        }
    }

    pub(super) fn cleanup(&mut self) {
        if self.completed {
            return;
        }

        #[cfg(unix)]
        {
            if let Ok(mut guard) = self.ctrlc.active_pgid.lock() {
                *guard = None;
            }
        }

        if let Some(handle) = self.stdout_handle.take()
            && let Err(e) = handle.join()
        {
            log::debug!("Stdout reader thread panicked: {:?}", e);
        }

        if let Some(handle) = self.stderr_handle.take()
            && let Err(e) = handle.join()
        {
            log::debug!("Stderr reader thread panicked: {:?}", e);
        }

        self.completed = true;
    }
}

impl Drop for ProcessCleanupGuard<'_> {
    fn drop(&mut self) {
        self.cleanup();
    }
}
