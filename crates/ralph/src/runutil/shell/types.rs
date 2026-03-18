//! Purpose: Define managed subprocess command, timeout, output, and error types.
//!
//! Responsibilities:
//! - Describe safe argv-only command input for operational subprocesses.
//! - Centralize timeout-class policy, capture limits, and command builder state.
//! - Provide managed subprocess output and failure types shared across shell helpers.
//!
//! Scope:
//! - Type definitions and timeout/capture policy only.
//!
//! Usage:
//! - Used by shell execution, wait, and test helpers.
//!
//! Invariants/Assumptions:
//! - Timeout classes remain the source of truth for default subprocess deadlines.
//! - Managed commands retain bounded capture settings and optional cancellation handles.
//! - Managed subprocess errors preserve descriptive context and captured-tail metadata.

use std::process::{Command, ExitStatus, Output};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use thiserror::Error;

use crate::constants::{buffers, timeouts};

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

    pub(super) fn capture_limits(self) -> CaptureLimits {
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
    pub(super) command: Command,
    pub(super) description: String,
    pub(super) timeout_class: TimeoutClass,
    pub(super) timeout_override: Option<Duration>,
    pub(super) capture_limits: CaptureLimits,
    pub(super) cancellation: Option<Arc<AtomicBool>>,
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
pub(super) enum TerminationReason {
    Timeout,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum RetryWaitError {
    #[error("retry wait cancelled")]
    Cancelled,
}
