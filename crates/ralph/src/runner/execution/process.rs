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
//!   On non-unix platforms, the grace period is also 2 seconds for consistency,
//!   though the mechanism differs (process.kill() vs signals).
//! - Process state transitions are: Running → TimeoutInterrupt/CtrlCInterrupt → (Exited | Killed).
//! - Ctrl-C interrupts are handled separately from timeout interrupts to ensure
//!   proper error propagation (Interrupted vs Timeout errors).
//! - Timeout errors take precedence over Interrupted errors when both occur:
//!   if a timeout triggers and then Ctrl-C is pressed during the grace period,
//!   the Timeout error is returned to ensure safeguard dumps/revert are triggered.

use std::io::Write;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::super::{
    OutputHandler, OutputStream, RunnerError, RunnerOutput, runner_execution_error,
    runner_execution_error_with_source,
};
use super::json::extract_session_id_from_text;
use super::stream::{StreamSink, spawn_json_reader, spawn_reader};

/// Type alias for reader thread join handle results.
type ReaderResult = anyhow::Result<()>;
use crate::contracts::Runner;

/// Guard that ensures cleanup runs on all exit paths from process execution.
///
/// This guard manages:
/// - Clearing `ctrlc.active_pgid` to prevent stale process group references
/// - Joining stdout/stderr reader threads to prevent output loss
///
/// Cleanup runs regardless of whether execution succeeded, timed out, or failed.
struct ProcessCleanupGuard<'a> {
    ctrlc: &'a CtrlCState,
    stdout_handle: Option<thread::JoinHandle<ReaderResult>>,
    stderr_handle: Option<thread::JoinHandle<ReaderResult>>,
    completed: bool,
}

impl<'a> ProcessCleanupGuard<'a> {
    fn new(
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

    #[allow(dead_code)]
    /// Mark cleanup as completed successfully (no error to report).
    fn mark_completed(&mut self) {
        self.completed = true;
    }

    /// Run cleanup and return any error encountered.
    /// This is idempotent - safe to call multiple times.
    fn cleanup(&mut self) {
        if self.completed {
            return;
        }

        // Clear active_pgid first (most critical to prevent sending signals to wrong process)
        #[cfg(unix)]
        {
            if let Ok(mut guard) = self.ctrlc.active_pgid.lock() {
                *guard = None;
            }
            // If lock fails, log but don't fail - we still need to join threads
        }

        // Join stdout reader if still pending
        if let Some(handle) = self.stdout_handle.take() {
            let _ = handle.join();
        }

        // Join stderr reader if still pending
        if let Some(handle) = self.stderr_handle.take() {
            let _ = handle.join();
        }

        self.completed = true;
    }
}

impl<'a> Drop for ProcessCleanupGuard<'a> {
    fn drop(&mut self) {
        // Ensure cleanup runs even if the guard is dropped without explicit cleanup call
        self.cleanup();
    }
}

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
///
/// This function is idempotent - it returns the existing state on subsequent calls
/// after the first successful initialization.
///
/// # Errors
/// Returns an error if the ctrlc handler cannot be installed (e.g., if another
/// handler is already registered).
pub(crate) fn ctrlc_state() -> Result<&'static Arc<CtrlCState>, CtrlCInitError> {
    static STATE: OnceLock<Arc<CtrlCState>> = OnceLock::new();

    // Fast path: already initialized
    if let Some(state) = STATE.get() {
        return Ok(state);
    }

    // Slow path: need to initialize
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
            .ok()
            .and_then(|guard| *guard);
        if let Some(pgid) = pgid {
            #[cfg(unix)]
            unsafe {
                libc::kill(-pgid, libc::SIGINT);
            }
        }
    }) {
        Ok(()) => {
            // Only set in OnceLock on success
            let state_ref = STATE.get_or_init(|| state);
            Ok(state_ref)
        }
        Err(e) => Err(CtrlCInitError {
            message: format!("Failed to install Ctrl-C handler: {}", e),
        }),
    }
}

/// Test-only helper to create a CtrlCState without registering a global handler.
///
/// This allows tests to have isolated Ctrl-C state without interfering with
/// the global handler registration.
#[cfg(test)]
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
/// 1. SIGINT is sent to the process group (graceful shutdown request) on unix,
///    or `child.kill()` is called on non-unix.
/// 2. A 2-second grace period allows the process to exit cleanly.
/// 3. If still running after grace period, SIGKILL is sent (forceful termination) on unix,
///    or a second `child.kill()` is issued on non-unix.
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
    #[cfg(not(unix))]
    let mut kill_requested = false;

    loop {
        let now = Instant::now();

        // Check for timeout and send interrupt if needed
        if let Some(timeout) = timeout
            && now.duration_since(start) > timeout
            && state == ProcessState::Running
        {
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
                // On non-unix, request kill but allow grace period before forcing
                let _ = child.kill();
                kill_requested = true;
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
                // On non-unix, request kill but allow grace period before forcing
                let _ = child.kill();
                kill_requested = true;
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
                // On non-unix, the first kill() was already called; we call it again
                // to ensure the process is terminated after the grace period.
                // Process may already be gone, so we ignore errors.
                #[cfg(not(unix))]
                {
                    let _ = child.kill();
                    kill_requested = true;
                }
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
                // On non-unix, if we requested kill but haven't escalated to Killed state yet,
                // keep waiting for the grace period to expire.
                #[cfg(not(unix))]
                if kill_requested
                    && matches!(
                        state,
                        ProcessState::TimeoutInterrupt(_) | ProcessState::CtrlCInterrupt(_)
                    )
                {
                    // Process is still running after kill request, wait for grace period
                }
            }
            Err(e) => return Err(RunnerError::Io(e)),
        }

        thread::sleep(Duration::from_millis(50));
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

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            let _ = libc::setpgid(0, 0);
            Ok(())
        });
    }

    let ctrlc = ctrlc_state().map_err(|e| RunnerError::Other(anyhow::anyhow!(e)))?;

    // Check for pre-run interrupt BEFORE resetting the flag.
    // If an interrupt was already pending, we should not proceed.
    if ctrlc.interrupted.load(Ordering::SeqCst) {
        return Err(RunnerError::Interrupted);
    }

    // Now safe to reset for this run
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
        let pid = child.id() as i32;
        *guard = Some(pid);
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

    // Create cleanup guard to ensure cleanup runs on ALL exit paths
    let mut guard = ProcessCleanupGuard::new(ctrlc, stdout_handle, stderr_handle);

    // Wait for child - this may return Ok, Timeout, Io, or other errors
    let status_result = wait_for_child(&mut child, ctrlc, timeout);

    // Extract buffers before cleanup (cleanup joins threads which completes the buffers)
    // We need to join the threads to get the full output
    guard.cleanup();

    // Now safe to extract buffers after threads are joined
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

    // Error precedence: Timeout takes priority over Interrupted.
    //
    // If a timeout occurred, we must return Timeout error so callers can trigger
    // safeguard dumps and git revert. This ensures that even if the user presses
    // Ctrl-C during the timeout grace period, the timeout handling is preserved.
    //
    // Only check for Interrupted if there was no timeout error.
    match &status_result {
        Err(RunnerError::Timeout) => {
            // Timeout occurred - return it regardless of Ctrl-C state
            return Err(RunnerError::Timeout);
        }
        _ => {
            // No timeout error - check if we were interrupted
            if ctrlc.interrupted.load(Ordering::SeqCst) {
                return Err(RunnerError::Interrupted);
            }
        }
    }

    // Now handle the wait_for_child result (either Ok or non-Timeout Err)
    let status = status_result?;

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
