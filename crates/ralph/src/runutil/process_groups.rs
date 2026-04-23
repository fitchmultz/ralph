//! Purpose: Share child-process group isolation helpers across subprocess launchers.
//!
//! Responsibilities:
//! - Attach `std::process::Command` values to a dedicated Unix process group.
//! - Centralize the `pre_exec` safety invariant for `setpgid(0, 0)` launch hooks.
//! - Compile as a no-op on non-Unix platforms so shared call sites stay simple.
//!
//! Scope:
//! - Child process-group setup only.
//!
//! Usage:
//! - Called by managed shell execution, runner execution, and parallel worker spawning.
//!
//! Invariants/Assumptions:
//! - Unix callers want the child to become the leader of a fresh process group.
//! - The `pre_exec` closure must remain async-signal-safe and must not touch shared Rust state.

use std::process::Command;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub(crate) fn isolate_child_process_group(command: &mut Command) {
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs in the forked child before `exec`; this closure
    // only makes the async-signal-safe `setpgid(0, 0)` call and does not touch
    // shared Rust state.
    unsafe {
        command.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    #[cfg(not(unix))]
    let _ = command;
}
