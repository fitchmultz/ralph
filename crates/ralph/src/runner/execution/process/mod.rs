//! Process and streaming orchestration for runner execution.
//!
//! Responsibilities:
//! - Spawn runner processes, stream stdout/stderr, and capture session output.
//! - Coordinate Ctrl-C handling and timeout transitions for runner execution.
//! - Delegate cleanup and wait-state handling to focused helper modules.
//!
//! Does not handle:
//! - Building runner command arguments (see `command` module).
//! - Rendering JSON events (see `stream_*` modules).
//!
//! Assumptions/invariants:
//! - Callers provide a fully constructed `Command` and validated runner/bin.
//! - Streaming threads must be joined before buffers are consumed.
//! - Unix subprocesses execute in isolated process groups so signals can target the full tree.

mod cleanup;
mod wait;

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use super::super::{
    OutputHandler, OutputStream, RunnerError, RunnerOutput, runner_execution_error,
    runner_execution_error_with_source,
};
use super::json::extract_session_id_from_text;
use super::stream::{StreamSink, spawn_json_reader, spawn_reader};
use crate::contracts::Runner;
use crate::runutil::isolate_child_process_group;
use cleanup::ProcessCleanupGuard;
#[cfg(test)]
pub(crate) use wait::wait_for_child;

pub(crate) struct CtrlCState {
    pub(crate) active_pgid: Mutex<Option<i32>>,
    pub(crate) interrupted: AtomicBool,
}

/// Error type for Ctrl-C handler initialization failures.
#[derive(Debug)]
pub(crate) struct CtrlCInitError {
    pub message: String,
}

impl std::fmt::Display for CtrlCInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CtrlCInitError {}

/// Initialize the global Ctrl-C handler and return the shared state.
pub(crate) fn ctrlc_state() -> Result<&'static Arc<CtrlCState>, CtrlCInitError> {
    static STATE: OnceLock<Arc<CtrlCState>> = OnceLock::new();

    if let Some(state) = STATE.get() {
        return Ok(state);
    }

    let state = Arc::new(CtrlCState {
        active_pgid: Mutex::new(None),
        interrupted: AtomicBool::new(false),
    });

    let handler_state = Arc::clone(&state);
    match ctrlc::set_handler(move || {
        handler_state.interrupted.store(true, Ordering::SeqCst);
        let pgid = handler_state
            .active_pgid
            .lock()
            .inspect_err(|e| log::debug!("Ctrl-C handler: failed to lock active_pgid: {}", e))
            .ok()
            .and_then(|guard| *guard);
        if let Some(pgid) = pgid {
            #[cfg(unix)]
            // SAFETY: Sending SIGINT to a process group is a standard POSIX operation.
            unsafe {
                libc::kill(-pgid, libc::SIGINT);
            }
        }
    }) {
        Ok(()) => Ok(STATE.get_or_init(|| state)),
        Err(e) => Err(CtrlCInitError {
            message: format!("Failed to install Ctrl-C handler: {}", e),
        }),
    }
}

/// Test-only helper to create a CtrlCState without registering a global handler.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn test_ctrlc_state() -> Arc<CtrlCState> {
    Arc::new(CtrlCState {
        active_pgid: Mutex::new(None),
        interrupted: AtomicBool::new(false),
    })
}

pub(super) fn ensure_self_on_path(cmd: &mut Command) {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return,
    };
    let dir = match exe.parent() {
        Some(dir) => dir.to_path_buf(),
        None => return,
    };

    let mut paths = Vec::new();
    paths.push(dir);

    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }

    if let Ok(joined) = std::env::join_paths(paths) {
        cmd.env("PATH", joined);
    }
}

/// Stream JSON output with visual filtering.
pub(super) fn run_with_streaming_json(
    cmd: Command,
    stdin_payload: Option<&[u8]>,
    runner: Runner,
    bin: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    run_with_streaming_json_inner(
        cmd,
        stdin_payload,
        runner,
        bin,
        timeout,
        output_handler,
        output_stream,
    )
}

fn run_with_streaming_json_inner(
    mut cmd: Command,
    stdin_payload: Option<&[u8]>,
    runner: Runner,
    bin: &str,
    timeout: Option<Duration>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> Result<RunnerOutput, RunnerError> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin_payload.is_some() {
        cmd.stdin(Stdio::piped());
    }

    isolate_child_process_group(&mut cmd);

    let ctrlc = ctrlc_state().map_err(|e| RunnerError::Other(anyhow::anyhow!(e)))?;
    ctrlc.interrupted.store(false, Ordering::SeqCst);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            RunnerError::BinaryMissing {
                bin: bin.to_string(),
                source: e,
            }
        } else {
            RunnerError::SpawnFailed {
                bin: bin.to_string(),
                source: e,
            }
        }
    })?;

    if let Some(payload) = stdin_payload {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| runner_execution_error(&runner, bin, "open child stdin"))?;
        stdin.write_all(payload).map_err(RunnerError::Io)?;
        drop(stdin);
    }

    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().map_err(|err| {
            runner_execution_error_with_source(&runner, bin, "lock ctrl-c state", err)
        })?;
        *guard = Some(child.id() as i32);
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| runner_execution_error(&runner, bin, "capture child stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| runner_execution_error(&runner, bin, "capture child stderr"))?;

    let stdout_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let session_id_buf = Arc::new(Mutex::new(None));

    let stdout_handle = spawn_json_reader(
        stdout,
        StreamSink::Stdout,
        Arc::clone(&stdout_buf),
        output_handler.clone(),
        output_stream,
        Arc::clone(&session_id_buf),
    );
    let stderr_handle = spawn_reader(
        stderr,
        StreamSink::Stderr,
        Arc::clone(&stderr_buf),
        output_handler,
        output_stream,
    );

    let mut guard = ProcessCleanupGuard::new(ctrlc, stdout_handle, stderr_handle);
    let status_result = wait::wait_for_child(&mut child, ctrlc, timeout);
    guard.cleanup();

    let stdout = {
        let mut guard = stdout_buf.lock().map_err(|err| {
            runner_execution_error_with_source(&runner, bin, "lock stdout buffer", err)
        })?;
        std::mem::take(&mut *guard)
    };
    let stderr = {
        let mut guard = stderr_buf.lock().map_err(|err| {
            runner_execution_error_with_source(&runner, bin, "lock stderr buffer", err)
        })?;
        std::mem::take(&mut *guard)
    };

    match &status_result {
        Err(RunnerError::Timeout) => return Err(RunnerError::Timeout),
        _ => {
            if ctrlc.interrupted.load(Ordering::SeqCst) {
                return Err(RunnerError::Interrupted);
            }
        }
    }

    let status = status_result?;
    let session_id = session_id_buf
        .lock()
        .inspect_err(|e| log::debug!("Failed to lock session_id_buf: {}", e))
        .ok()
        .and_then(|guard| guard.clone())
        .or_else(|| extract_session_id_from_text(&stdout));

    Ok(RunnerOutput {
        status,
        stdout,
        stderr,
        session_id,
    })
}
