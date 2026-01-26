//! Process and streaming orchestration for runner execution.
//!
//! Responsibilities:
//! - Spawn runner processes, stream stdout/stderr, and capture session output.
//! - Coordinate Ctrl-C handling and timeouts for runner execution.
//!
//! Does not handle:
//! - Building runner command arguments (see command module).
//! - Validating runner configuration or selecting tasks.
//!
//! Assumptions/invariants:
//! - Callers provide a fully constructed `Command` and validated runner/bin.
//! - Streaming threads must be joined to avoid output loss.

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

pub(super) struct CtrlCState {
    pub(super) active_pgid: Mutex<Option<i32>>,
    pub(super) interrupted: AtomicBool,
}

pub(super) fn ctrlc_state() -> &'static Arc<CtrlCState> {
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

pub(super) fn wait_for_child(
    child: &mut std::process::Child,
    ctrlc: &CtrlCState,
    timeout: Option<Duration>,
) -> Result<ExitStatus, RunnerError> {
    let mut interrupt_sent = false;
    let mut kill_sent = false;
    let start = Instant::now();
    let mut interrupt_time = None;

    loop {
        let now = Instant::now();

        if let Some(timeout) = timeout {
            if now.duration_since(start) > timeout && !interrupt_sent {
                log::warn!("Runner timed out after {:?}; sending interrupt", timeout);
                interrupt_sent = true;
                interrupt_time = Some(now);
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

        if ctrlc.interrupted.load(Ordering::SeqCst) && !interrupt_sent {
            interrupt_sent = true;
            interrupt_time = Some(now);
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

        if interrupt_sent && !kill_sent {
            let elapsed_since_interrupt = now.duration_since(interrupt_time.unwrap());
            if elapsed_since_interrupt > Duration::from_secs(2) {
                kill_sent = true;
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

        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(timeout) = timeout {
                    if now.duration_since(start) > timeout {
                        return Err(RunnerError::Timeout);
                    }
                }
                return Ok(status);
            }
            Ok(None) => {}
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
