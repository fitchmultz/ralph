//! Process and streaming orchestration for runner execution.
//!
//! Responsibilities:
//! - Spawn runner processes, stream stdout/stderr, and capture session output.
//! - Coordinate Ctrl-C handling and timeouts for runner execution.
//! - Handle timeout race conditions: if a process exits cleanly (code 0) after timeout
//!   interrupt, it is treated as success rather than timeout failure.
//!
//! Does not handle:
//! - Building runner command arguments (see command module).
//! - Validating runner configuration or selecting tasks.
//!
//! Assumptions/invariants:
//! - Callers provide a fully constructed `Command` and validated runner/bin.
//! - Streaming threads must be joined to avoid output loss.
//! - Timeout handling uses a 2-second grace period after SIGINT before SIGKILL.
//! - Process state transitions are: Running → TimeoutInterrupt/CtrlCInterrupt → (Exited | Killed).
//! - Ctrl-C interrupts are handled separately from timeout interrupts to ensure
//!   proper error propagation (Interrupted vs Timeout errors).

use std::io::Write;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::super::{
    runner_execution_error, runner_execution_error_with_source, OutputHandler, OutputStream,
    RunnerError, RunnerOutput,
};
use super::json::extract_session_id_from_text;
use super::stream::{spawn_json_reader, spawn_reader, StreamSink};
use crate::contracts::Runner;

pub(crate) struct CtrlCState {
    pub(crate) active_pgid: Mutex<Option<i32>>,
    pub(crate) interrupted: AtomicBool,
}

pub(crate) fn ctrlc_state() -> &'static Arc<CtrlCState> {
    static STATE: OnceLock<Arc<CtrlCState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let state = Arc::new(CtrlCState {
            active_pgid: Mutex::new(None),
            interrupted: AtomicBool::new(false),
        });
        let handler_state = Arc::clone(&state);
        let _ = ctrlc::set_handler(move || {
            handler_state.interrupted.store(true, Ordering::SeqCst);
            let pgid = handler_state
                .active_pgid
                .lock()
                .ok()
                .and_then(|guard| *guard);
            if let Some(pgid) = pgid {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-pgid, libc::SIGINT);
                }
            }
        });
        state
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

/// Tracks the state of process termination handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessState {
    /// Process is running normally.
    Running,
    /// Process has been interrupted due to timeout (SIGINT sent) at the given time.
    TimeoutInterrupt(Instant),
    /// Process has been interrupted due to Ctrl-C (SIGINT sent) at the given time.
    CtrlCInterrupt(Instant),
    /// Process has been killed (SIGKILL sent).
    Killed,
}

/// Waits for a child process to exit, handling timeouts and Ctrl-C interrupts.
///
/// # Timeout Handling
///
/// When a timeout is specified and the process exceeds it:
/// 1. SIGINT is sent to the process group (graceful shutdown request)
/// 2. A 2-second grace period allows the process to exit cleanly
/// 3. If still running after grace period, SIGKILL is sent (forceful termination)
///
/// # Race Condition Fix
///
/// Previously, if a process exited *after* the timeout threshold but *before*
/// `try_wait()` was called, the function would incorrectly return `Timeout`
/// even though the process succeeded. Now, if the process exits with code 0
/// after receiving the interrupt signal, `Ok(status)` is returned instead of
/// `Err(RunnerError::Timeout)`.
///
/// If the process exits non-zero or is killed after timeout/interrupt,
/// `Err(RunnerError::Timeout)` is returned so callers can handle safeguard
/// dumps and git revert appropriately.
pub(crate) fn wait_for_child(
    child: &mut std::process::Child,
    ctrlc: &CtrlCState,
    timeout: Option<Duration>,
) -> Result<ExitStatus, RunnerError> {
    let mut state = ProcessState::Running;
    let start = Instant::now();

    loop {
        let now = Instant::now();

        // Check for timeout and send interrupt if needed
        if let Some(timeout) = timeout {
            if now.duration_since(start) > timeout && state == ProcessState::Running {
                log::warn!("Runner timed out after {:?}; sending interrupt", timeout);
                state = ProcessState::TimeoutInterrupt(now);
                #[cfg(unix)]
                {
                    let pgid = ctrlc.active_pgid.lock().ok().and_then(|guard| *guard);
                    if let Some(pgid) = pgid {
                        unsafe {
                            libc::kill(-pgid, libc::SIGINT);
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = child.kill();
                }
            }
        }

        // Check for Ctrl-C interrupt
        if ctrlc.interrupted.load(Ordering::SeqCst) && state == ProcessState::Running {
            log::debug!("Ctrl-C detected; sending interrupt to runner process");
            state = ProcessState::CtrlCInterrupt(now);
            #[cfg(unix)]
            {
                let pgid = ctrlc.active_pgid.lock().ok().and_then(|guard| *guard);
                if let Some(pgid) = pgid {
                    unsafe {
                        libc::kill(-pgid, libc::SIGINT);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                let _ = child.kill();
            }
        }

        // Check if we need to escalate to kill after grace period
        let interrupt_time = match state {
            ProcessState::TimeoutInterrupt(t) | ProcessState::CtrlCInterrupt(t) => Some(t),
            _ => None,
        };
        if let Some(interrupt_time) = interrupt_time {
            let elapsed_since_interrupt = now.duration_since(interrupt_time);
            if elapsed_since_interrupt > Duration::from_secs(2) {
                log::debug!(
                    "Interrupt grace period expired (2s); sending SIGKILL to runner process"
                );
                state = ProcessState::Killed;
                #[cfg(unix)]
                {
                    let pgid = ctrlc.active_pgid.lock().ok().and_then(|guard| *guard);
                    if let Some(pgid) = pgid {
                        unsafe {
                            libc::kill(-pgid, libc::SIGKILL);
                        }
                    }
                }
                let _ = child.kill();
            }
        }

        // Check process status
        match child.try_wait() {
            Ok(Some(status)) => {
                // Race condition fix: if the process exited successfully (code 0) after
                // receiving an interrupt (timeout or Ctrl-C), treat it as success
                // rather than returning Timeout or Interrupted errors.
                //
                // If the process exited non-zero after timeout interrupt, return Timeout error
                // so callers can handle safeguard dumps and git revert appropriately.
                // If the process exited non-zero after Ctrl-C, return the actual exit status
                // and let run_with_streaming_json handle the Interrupted error.
                if status.success() {
                    match state {
                        ProcessState::TimeoutInterrupt(_) | ProcessState::CtrlCInterrupt(_) => {
                            log::debug!(
                                "Process exited cleanly (code 0) after interrupt; treating as success"
                            );
                            return Ok(status);
                        }
                        ProcessState::Killed => {
                            // Process was killed but exited 0 - still treat as success
                            log::debug!(
                                "Process exited cleanly (code 0) after kill; treating as success"
                            );
                            return Ok(status);
                        }
                        ProcessState::Running => {
                            return Ok(status);
                        }
                    }
                }

                // Process exited with non-zero status
                if let Some(code) = status.code() {
                    log::debug!("Process exited with non-zero code: {}", code);
                } else {
                    log::debug!("Process terminated by signal");
                }

                // If the process was interrupted due to timeout or killed,
                // return Timeout error so callers can handle safeguard dumps.
                // For Ctrl-C, return the actual exit status and let
                // run_with_streaming_json check ctrlc.interrupted and return Interrupted.
                match state {
                    ProcessState::TimeoutInterrupt(_) | ProcessState::Killed => {
                        return Err(RunnerError::Timeout);
                    }
                    ProcessState::CtrlCInterrupt(_) | ProcessState::Running => {
                        return Ok(status);
                    }
                }
            }
            Ok(None) => {
                // Process still running, continue loop
            }
            Err(e) => return Err(RunnerError::Io(e)),
        }

        thread::sleep(Duration::from_millis(50));
    }
}

/// Stream JSON output with visual filtering.
pub(super) fn run_with_streaming_json(
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

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let ctrlc = ctrlc_state();
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
            .ok_or_else(|| runner_execution_error(runner, bin, "open child stdin"))?;
        stdin.write_all(payload).map_err(RunnerError::Io)?;
        drop(stdin);
    }

    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().map_err(|err| {
            runner_execution_error_with_source(runner, bin, "lock ctrl-c state", err)
        })?;
        let pid = child.id() as i32;
        *guard = Some(pid);
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| runner_execution_error(runner, bin, "capture child stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| runner_execution_error(runner, bin, "capture child stderr"))?;

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

    let status = wait_for_child(&mut child, ctrlc, timeout)?;

    #[cfg(unix)]
    {
        let mut guard = ctrlc.active_pgid.lock().map_err(|err| {
            runner_execution_error_with_source(runner, bin, "lock ctrl-c state", err)
        })?;
        *guard = None;
    }

    stdout_handle
        .join()
        .map_err(|_| runner_execution_error(runner, bin, "stdout reader panicked"))?
        .map_err(|err| {
            runner_execution_error_with_source(runner, bin, "read stdout stream", err)
        })?;
    stderr_handle
        .join()
        .map_err(|_| runner_execution_error(runner, bin, "stderr reader panicked"))?
        .map_err(|err| {
            runner_execution_error_with_source(runner, bin, "read stderr stream", err)
        })?;

    let stdout = {
        let mut guard = stdout_buf.lock().map_err(|err| {
            runner_execution_error_with_source(runner, bin, "lock stdout buffer", err)
        })?;
        std::mem::take(&mut *guard)
    };
    let stderr = {
        let mut guard = stderr_buf.lock().map_err(|err| {
            runner_execution_error_with_source(runner, bin, "lock stderr buffer", err)
        })?;
        std::mem::take(&mut *guard)
    };

    if ctrlc.interrupted.load(Ordering::SeqCst) {
        return Err(RunnerError::Interrupted);
    }

    let session_id = session_id_buf
        .lock()
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
