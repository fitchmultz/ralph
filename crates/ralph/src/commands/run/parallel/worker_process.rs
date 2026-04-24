//! Worker subprocess lifecycle helpers.
//!
//! Purpose:
//! - Worker subprocess lifecycle helpers.
//!
//! Responsibilities:
//! - Spawn worker subprocesses in isolated workspaces.
//! - Emit explicit worker-exit events instead of requiring orchestrators to poll children.
//! - Terminate workers gracefully and wait for monitor confirmation during cleanup.
//!
//! Non-scope:
//! - Task selection.
//! - Parallel orchestration loop state transitions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Worker commands create isolated process groups on unix.
//! - Each worker has exactly one monitor thread that owns `Child::wait()`.

use crate::agent::AgentOverrides;
use crate::config;
use crate::git::WorkspaceSpec;
#[cfg(windows)]
use crate::runutil::{ManagedCommand, TimeoutClass, execute_managed_command};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
#[cfg(windows)]
use std::process::Command;
use std::process::{Child, ExitStatus};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use super::command::{build_worker_command, debug_command_args};

const WORKER_INTERRUPT_GRACE: Duration = Duration::from_millis(1_500);
const WORKER_EXIT_WAIT_SLICE: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(crate) struct FinishedWorker {
    pub task_id: String,
    pub task_title: String,
    pub workspace: WorkspaceSpec,
    pub status: ExitStatus,
}

pub(crate) struct WorkerState {
    pub task_id: String,
    pub pid: u32,
    exit_rx: Receiver<std::io::Result<ExitStatus>>,
}

impl WorkerState {
    fn recv_exit_timeout(&self, timeout: Duration) -> Option<std::io::Result<ExitStatus>> {
        match self.exit_rx.recv_timeout(timeout) {
            Ok(status) => Some(status),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => Some(Err(std::io::Error::other(
                "worker exit monitor disconnected",
            ))),
        }
    }

    fn terminate(&mut self) {
        #[cfg(unix)]
        {
            request_worker_interrupt(self.pid, &self.task_id);
            if self.recv_exit_timeout(WORKER_INTERRUPT_GRACE).is_some() {
                return;
            }

            force_kill_worker(self.pid, &self.task_id);
            let _ = self.recv_exit_timeout(WORKER_EXIT_WAIT_SLICE);
        }

        #[cfg(windows)]
        {
            terminate_worker_process_windows(self.pid, &self.task_id);
            let _ = self.recv_exit_timeout(WORKER_INTERRUPT_GRACE + WORKER_EXIT_WAIT_SLICE);
            return;
        }

        #[cfg(all(not(unix), not(windows)))]
        {
            log::warn!(
                "Worker {} has no explicit termination support on this platform; waiting for exit only.",
                self.task_id
            );
            let _ = self.recv_exit_timeout(WORKER_INTERRUPT_GRACE + WORKER_EXIT_WAIT_SLICE);
        }
    }
}

pub(crate) fn terminate_workers(in_flight: &mut HashMap<String, WorkerState>) {
    for worker in in_flight.values_mut() {
        worker.terminate();
    }
}

#[cfg(unix)]
fn request_worker_interrupt(pid: u32, task_id: &str) {
    send_signal(pid as i32, libc::SIGINT, task_id, "SIGINT");
}

#[cfg(unix)]
fn force_kill_worker(pid: u32, task_id: &str) {
    send_signal(pid as i32, libc::SIGKILL, task_id, "SIGKILL");
}

#[cfg(unix)]
fn send_signal(pid: i32, signal: i32, task_id: &str, label: &str) {
    let group_result = unsafe { libc::kill(-pid, signal) };
    if group_result == 0 {
        return;
    }

    let group_err = std::io::Error::last_os_error();
    let direct_result = unsafe { libc::kill(pid, signal) };
    if direct_result == 0 {
        return;
    }

    let direct_err = std::io::Error::last_os_error();
    if direct_err.raw_os_error() != Some(libc::ESRCH) {
        log::warn!(
            "Failed to send {} to worker {} via pgid {} ({}) and pid {} ({}).",
            label,
            task_id,
            pid,
            group_err,
            pid,
            direct_err
        );
    }
}

#[cfg(windows)]
fn terminate_worker_process_windows(pid: u32, task_id: &str) {
    let mut command = Command::new("taskkill");
    command.args(["/PID", &pid.to_string(), "/T", "/F"]);
    match execute_managed_command(ManagedCommand::new(
        command,
        format!("taskkill worker {task_id}"),
        TimeoutClass::Probe,
    )) {
        Ok(output) if output.status.success() => {}
        Ok(output) => log::warn!(
            "Failed to terminate worker {} with taskkill (status: {}, stderr: {}).",
            task_id,
            output.status,
            output.stderr_lossy()
        ),
        Err(err) => log::warn!("Failed to launch taskkill for worker {}: {}", task_id, err),
    }
}

pub(crate) fn spawn_worker(
    resolved: &config::Resolved,
    workspace_path: &Path,
    task_id: &str,
    target_branch: &str,
    overrides: &AgentOverrides,
    force: bool,
) -> Result<Child> {
    let mut cmd = build_worker_command(
        resolved,
        workspace_path,
        task_id,
        target_branch,
        overrides,
        force,
    )?;
    log::debug!(
        "Spawning parallel worker {} in {} with args: {:?}",
        task_id,
        workspace_path.display(),
        debug_command_args(&cmd)
    );
    cmd.spawn().context("spawn parallel worker")
}

pub(crate) fn start_worker_monitor(
    task_id: &str,
    task_title: String,
    workspace: WorkspaceSpec,
    mut child: Child,
    worker_events: Sender<FinishedWorker>,
) -> WorkerState {
    let pid = child.id();
    let task_id_owned = task_id.to_string();
    let (exit_tx, exit_rx) = mpsc::channel();
    let event_task_id = task_id_owned.clone();
    let event_title = task_title.clone();
    let event_workspace = workspace.clone();

    thread::spawn(move || {
        let result = child.wait();
        match result {
            Ok(status) => {
                let _ = worker_events.send(FinishedWorker {
                    task_id: event_task_id.clone(),
                    task_title: event_title,
                    workspace: event_workspace,
                    status,
                });
                let _ = exit_tx.send(Ok(status));
            }
            Err(err) => {
                let _ = exit_tx.send(Err(err));
            }
        }
    });

    WorkerState {
        task_id: task_id_owned,
        pid,
        exit_rx,
    }
}
