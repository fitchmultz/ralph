//! Managed subprocess execution for non-runner commands.
//!
//! Responsibilities:
//! - Provide one argv-first subprocess service for CI, plugins, git/gh, doctor, and probes.
//! - Enforce timeout classes, bounded stdout/stderr capture, and signal escalation.
//! - Offer lightweight cancellation-aware waiting and retry-friendly command metadata.
//!
//! Not handled here:
//! - Runner streaming/json protocol execution (see `crate::runner::execution`).
//! - Domain-specific error classification for git/gh/CI/plugin failures.
//!
//! Invariants/assumptions:
//! - Direct argv execution is the only supported CI gate execution path.
//! - Managed subprocesses run with piped stdout/stderr and bounded in-memory capture.
//! - On unix, managed subprocesses execute in isolated process groups so SIGINT/SIGKILL can
//!   target the full subtree rather than only the immediate child.

use anyhow::{Context, Result, bail};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::constants::{buffers, timeouts};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Structured command execution without implicit shell interpolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SafeCommand {
    Argv { argv: Vec<String> },
}

/// Timeout category for managed subprocesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeoutClass {
    Probe,
    MetadataProbe,
    Git,
    GitHubCli,
    PluginHook,
    AppLaunch,
    MediaPlayback,
    CiGate,
}

impl TimeoutClass {
    pub(crate) fn timeout(self) -> Duration {
        match self {
            Self::Probe => timeouts::MANAGED_SUBPROCESS_PROBE_TIMEOUT,
            Self::MetadataProbe => timeouts::MANAGED_SUBPROCESS_METADATA_TIMEOUT,
            Self::Git => timeouts::MANAGED_SUBPROCESS_GIT_TIMEOUT,
            Self::GitHubCli => timeouts::MANAGED_SUBPROCESS_GH_TIMEOUT,
            Self::PluginHook => timeouts::MANAGED_SUBPROCESS_PLUGIN_TIMEOUT,
            Self::AppLaunch => timeouts::MANAGED_SUBPROCESS_APP_LAUNCH_TIMEOUT,
            Self::MediaPlayback => timeouts::MANAGED_SUBPROCESS_MEDIA_PLAYBACK_TIMEOUT,
            Self::CiGate => timeouts::MANAGED_SUBPROCESS_CI_TIMEOUT,
        }
    }

    fn capture_limits(self) -> CaptureLimits {
        match self {
            Self::CiGate => CaptureLimits {
                stdout_max_bytes: buffers::MANAGED_SUBPROCESS_CI_CAPTURE_MAX_BYTES,
                stderr_max_bytes: buffers::MANAGED_SUBPROCESS_CI_CAPTURE_MAX_BYTES,
            },
            _ => CaptureLimits {
                stdout_max_bytes: buffers::MANAGED_SUBPROCESS_CAPTURE_MAX_BYTES,
                stderr_max_bytes: buffers::MANAGED_SUBPROCESS_CAPTURE_MAX_BYTES,
            },
        }
    }
}

/// Output capture bounds for a managed subprocess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptureLimits {
    pub stdout_max_bytes: usize,
    pub stderr_max_bytes: usize,
}

/// Configured managed subprocess invocation.
#[derive(Debug)]
pub(crate) struct ManagedCommand {
    command: Command,
    description: String,
    timeout_class: TimeoutClass,
    timeout_override: Option<Duration>,
    capture_limits: CaptureLimits,
    cancellation: Option<Arc<AtomicBool>>,
}

impl ManagedCommand {
    pub(crate) fn new(
        command: Command,
        description: impl Into<String>,
        timeout_class: TimeoutClass,
    ) -> Self {
        Self {
            command,
            description: description.into(),
            timeout_class,
            timeout_override: None,
            capture_limits: timeout_class.capture_limits(),
            cancellation: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout_override = Some(timeout);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_capture_limits(mut self, capture_limits: CaptureLimits) -> Self {
        self.capture_limits = capture_limits;
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_cancellation(mut self, cancellation: Arc<AtomicBool>) -> Self {
        self.cancellation = Some(cancellation);
        self
    }
}

/// Managed subprocess outcome with bounded capture metadata.
#[derive(Debug, Clone)]
pub(crate) struct ManagedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

impl ManagedOutput {
    pub(crate) fn into_output(self) -> Output {
        Output {
            status: self.status,
            stdout: self.stdout,
            stderr: self.stderr,
        }
    }

    pub(crate) fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_string()
    }

    pub(crate) fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_string()
    }
}

/// Failure while executing a managed subprocess.
#[derive(Debug, Error)]
pub(crate) enum ManagedProcessError {
    #[error("failed to spawn managed subprocess '{description}': {source}")]
    Spawn {
        description: String,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "managed subprocess '{description}' timed out after {timeout:?} (stdout_tail={stdout_tail} bytes, stderr_tail={stderr_tail} bytes)"
    )]
    TimedOut {
        description: String,
        timeout: Duration,
        stdout_tail: usize,
        stderr_tail: usize,
    },
    #[error(
        "managed subprocess '{description}' was cancelled (stdout_tail={stdout_tail} bytes, stderr_tail={stderr_tail} bytes)"
    )]
    Cancelled {
        description: String,
        stdout_tail: usize,
        stderr_tail: usize,
    },
    #[error("failed while waiting for managed subprocess '{description}': {source}")]
    Wait {
        description: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminationReason {
    Timeout,
    Cancelled,
}

#[derive(Debug)]
struct BoundedCapture {
    bytes: Vec<u8>,
    max_bytes: usize,
    truncated: bool,
}

impl BoundedCapture {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
            truncated: false,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        if self.max_bytes == 0 {
            self.truncated = true;
            return;
        }
        if chunk.len() >= self.max_bytes {
            self.bytes.clear();
            self.bytes
                .extend_from_slice(&chunk[chunk.len() - self.max_bytes..]);
            self.truncated = true;
            return;
        }

        let next_len = self.bytes.len() + chunk.len();
        if next_len > self.max_bytes {
            let excess = next_len - self.max_bytes;
            self.bytes.drain(..excess);
            self.truncated = true;
        }
        self.bytes.extend_from_slice(chunk);
    }
}

pub(crate) fn execute_safe_command(safe_command: &SafeCommand, cwd: &Path) -> Result<Output> {
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
    // SAFETY: `pre_exec` only calls `setpgid(0, 0)` to isolate the child process group.
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
    let outcome = wait_for_child(
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

pub(crate) fn sleep_with_cancellation(
    duration: Duration,
    cancellation: Option<&AtomicBool>,
) -> std::result::Result<(), RetryWaitError> {
    if cancellation.is_none() {
        thread::sleep(duration);
        return Ok(());
    }

    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if cancellation.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err(RetryWaitError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining.min(timeouts::MANAGED_RETRY_POLL_INTERVAL));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum RetryWaitError {
    #[error("retry wait cancelled")]
    Cancelled,
}

fn build_argv_command(argv: &[String]) -> Result<Command> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("argv command must include at least one element"))?;
    if program.trim().is_empty() {
        bail!("argv command program must be non-empty.");
    }

    let mut command = Command::new(PathBuf::from(program));
    command.args(args);
    Ok(command)
}

fn spawn_capture_thread(
    mut reader: impl Read + Send + 'static,
    capture: Arc<Mutex<BoundedCapture>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut guard = capture
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    guard.push(&buf[..n]);
                }
                Err(err) => {
                    log::debug!(
                        "managed subprocess reader exiting after read error: {}",
                        err
                    );
                    break;
                }
            }
        }
    })
}

fn join_capture_thread(handle: thread::JoinHandle<()>) {
    if let Err(err) = handle.join() {
        log::debug!("managed subprocess capture thread panicked: {:?}", err);
    }
}

fn unwrap_capture(capture: Arc<Mutex<BoundedCapture>>) -> BoundedCapture {
    match Arc::try_unwrap(capture) {
        Ok(mutex) => mutex
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        Err(shared) => {
            let mut guard = shared
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            BoundedCapture {
                bytes: std::mem::take(&mut guard.bytes),
                max_bytes: guard.max_bytes,
                truncated: guard.truncated,
            }
        }
    }
}

#[cfg(unix)]
fn wait_for_child(
    child: std::process::Child,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
) -> std::io::Result<ManagedChildOutcome> {
    wait_for_child_unix(child, timeout, cancellation)
}

#[cfg(not(unix))]
fn wait_for_child(
    child: &mut std::process::Child,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
) -> std::io::Result<ManagedChildOutcome> {
    wait_for_child_polling(child, timeout, cancellation)
}

#[cfg(unix)]
fn wait_for_child_unix(
    child: std::process::Child,
    timeout: Duration,
    cancellation: Option<Arc<AtomicBool>>,
) -> std::io::Result<ManagedChildOutcome> {
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
fn wait_for_child_polling(
    child: &mut std::process::Child,
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

        thread::sleep(timeouts::MANAGED_SUBPROCESS_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn spawn_exit_waiter(mut child: std::process::Child) -> Receiver<std::io::Result<ExitStatus>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = child.wait();
        let _ = tx.send(result);
    });
    rx
}

#[cfg(unix)]
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

struct ManagedChildOutcome {
    status: ExitStatus,
    termination: Option<TerminationReason>,
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
fn signal_child(child: &mut std::process::Child, _hard_kill: bool) {
    let _ = child.kill();
}

#[cfg(unix)]
fn signal_pid(pid: u32, hard_kill: bool) {
    let signal = if hard_kill {
        libc::SIGKILL
    } else {
        libc::SIGINT
    };
    unsafe {
        let _ = libc::kill(-(pid as i32), signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn managed_command_bounds_captured_output() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("printf '1234567890abcdef'");
        let output = execute_managed_command(
            ManagedCommand::new(command, "capture", TimeoutClass::Probe).with_capture_limits(
                CaptureLimits {
                    stdout_max_bytes: 4,
                    stderr_max_bytes: 4,
                },
            ),
        )
        .expect("managed command should succeed");

        assert_eq!(String::from_utf8_lossy(&output.stdout), "cdef");
        assert!(output.stdout_truncated);
    }

    #[cfg(unix)]
    #[test]
    fn sleep_with_cancellation_stops_early() {
        let cancelled = AtomicBool::new(true);
        let result = sleep_with_cancellation(Duration::from_secs(1), Some(&cancelled));
        assert!(matches!(result, Err(RetryWaitError::Cancelled)));
    }

    #[cfg(unix)]
    #[test]
    fn managed_command_times_out() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("sleep 5");
        let err = execute_managed_command(
            ManagedCommand::new(command, "timeout", TimeoutClass::Probe)
                .with_timeout(Duration::from_millis(100)),
        )
        .expect_err("managed command should time out");

        assert!(matches!(err, ManagedProcessError::TimedOut { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn execute_checked_command_returns_stdout_on_success() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("printf 'ok'");
        let output = execute_checked_command(ManagedCommand::new(
            command,
            "checked success",
            TimeoutClass::MetadataProbe,
        ))
        .expect("managed command should succeed");

        assert_eq!(output.stdout_lossy(), "ok");
    }

    #[cfg(unix)]
    #[test]
    fn execute_checked_command_includes_stderr_on_failure() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("printf 'boom' >&2; exit 7");
        let err = execute_checked_command(ManagedCommand::new(
            command,
            "checked failure",
            TimeoutClass::AppLaunch,
        ))
        .expect_err("managed command should fail");

        let text = err.to_string();
        assert!(text.contains("checked failure failed"));
        assert!(text.contains("exit status"));
        assert!(text.contains("boom"));
    }
}
