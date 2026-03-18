//! Purpose: Execute managed subprocesses with bounded capture and timeout handling.
//!
//! Responsibilities:
//! - Run argv-only operational subprocesses without shell interpolation.
//! - Wire bounded stdout/stderr capture, timeout policy, and cancellation-aware waiting.
//! - Provide checked-status helpers that surface captured failure detail.
//!
//! Scope:
//! - Managed subprocess execution and status validation only.
//!
//! Usage:
//! - Used by CI, git/gh, plugin, doctor, and app-runtime operational helpers.
//!
//! Invariants/Assumptions:
//! - Managed subprocesses always run with piped stdout/stderr and bounded capture.
//! - Unix subprocesses run in isolated process groups before execution begins.
//! - Timeout and cancellation failures preserve tail-byte counts for diagnostics.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result, bail};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::sync::{Arc, Mutex};

use super::argv::build_argv_command;
use super::capture::{BoundedCapture, join_capture_thread, spawn_capture_thread, unwrap_capture};
use super::types::{
    ManagedCommand, ManagedOutput, ManagedProcessError, SafeCommand, TerminationReason,
    TimeoutClass,
};
#[cfg(not(unix))]
use super::wait::wait_for_child;
#[cfg(unix)]
use super::wait::wait_for_child_owned;

pub(crate) fn execute_safe_command(
    safe_command: &SafeCommand,
    cwd: &Path,
) -> Result<std::process::Output> {
    let (mut command, description) = match safe_command {
        SafeCommand::Argv { argv } => {
            let command = build_argv_command(argv)?;
            (command, argv.join(" "))
        }
    };

    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    execute_managed_command(ManagedCommand::new(
        command,
        description,
        TimeoutClass::CiGate,
    ))
    .map(ManagedOutput::into_output)
    .map_err(Into::into)
}

pub(crate) fn execute_managed_command(
    mut managed: ManagedCommand,
) -> std::result::Result<ManagedOutput, ManagedProcessError> {
    managed
        .command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    unsafe {
        managed.command.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let description = managed.description.clone();
    let timeout = managed
        .timeout_override
        .unwrap_or_else(|| managed.timeout_class.timeout());
    let mut child = managed
        .command
        .spawn()
        .map_err(|source| ManagedProcessError::Spawn {
            description: description.clone(),
            source,
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ManagedProcessError::Wait {
            description: description.clone(),
            source: std::io::Error::other("child stdout pipe was not available"),
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ManagedProcessError::Wait {
            description: description.clone(),
            source: std::io::Error::other("child stderr pipe was not available"),
        })?;

    let stdout_capture = Arc::new(Mutex::new(BoundedCapture::new(
        managed.capture_limits.stdout_max_bytes,
    )));
    let stderr_capture = Arc::new(Mutex::new(BoundedCapture::new(
        managed.capture_limits.stderr_max_bytes,
    )));
    let stdout_handle = spawn_capture_thread(stdout, Arc::clone(&stdout_capture));
    let stderr_handle = spawn_capture_thread(stderr, Arc::clone(&stderr_capture));

    #[cfg(unix)]
    let outcome = wait_for_child_owned(
        child,
        timeout,
        managed.cancellation.as_ref().map(Arc::clone),
    )
    .map_err(|source| ManagedProcessError::Wait {
        description: description.clone(),
        source,
    })?;

    #[cfg(not(unix))]
    let outcome = wait_for_child(
        &mut child,
        timeout,
        managed.cancellation.as_ref().map(Arc::clone),
    )
    .map_err(|source| ManagedProcessError::Wait {
        description: description.clone(),
        source,
    })?;

    join_capture_thread(stdout_handle);
    join_capture_thread(stderr_handle);

    let stdout_capture = unwrap_capture(stdout_capture);
    let stderr_capture = unwrap_capture(stderr_capture);

    if let Some(reason) = outcome.termination {
        return Err(match reason {
            TerminationReason::Timeout => ManagedProcessError::TimedOut {
                description,
                timeout,
                stdout_tail: stdout_capture.bytes.len(),
                stderr_tail: stderr_capture.bytes.len(),
            },
            TerminationReason::Cancelled => ManagedProcessError::Cancelled {
                description,
                stdout_tail: stdout_capture.bytes.len(),
                stderr_tail: stderr_capture.bytes.len(),
            },
        });
    }

    Ok(ManagedOutput {
        status: outcome.status,
        stdout: stdout_capture.bytes,
        stderr: stderr_capture.bytes,
        stdout_truncated: stdout_capture.truncated,
        stderr_truncated: stderr_capture.truncated,
    })
}

pub(crate) fn execute_checked_command(managed: ManagedCommand) -> anyhow::Result<ManagedOutput> {
    let description = managed.description.clone();
    let output = execute_managed_command(managed)
        .with_context(|| format!("execute managed subprocess `{description}`"))?;
    ensure_success_status(&description, &output)?;
    Ok(output)
}

pub(crate) fn ensure_success_status(
    description: &str,
    output: &ManagedOutput,
) -> anyhow::Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = output.stderr_lossy();
    let stdout = output.stdout_lossy();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "process exited without output".to_string()
    };

    bail!(
        "{description} failed (exit status: {}). {}",
        output.status,
        detail
    )
}
