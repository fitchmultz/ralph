//! Shared low-level support helpers for run-command tests.
//!
//! Purpose:
//! - Shared low-level support helpers for run-command tests.
//!
//! Responsibilities:
//! - Provide process-state helpers that are awkward to inline in individual suites.
//! - Keep lock- and PID-oriented probes centralized for reuse.
//!
//! Not handled here:
//! - Task fixtures or config builders.
//! - Production lock management logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Helpers remain deterministic enough for local CI.
//! - PID probes stay best-effort and return only confirmed-dead processes.

pub(crate) fn find_definitely_dead_pid() -> u32 {
    for pid in [0xFFFFFFFE, 999_999, 500_000, 250_000, 100_000] {
        if crate::lock::pid_is_running(pid) == Some(false) {
            return pid;
        }
    }
    panic!("Could not find a definitely-dead PID on this system");
}
