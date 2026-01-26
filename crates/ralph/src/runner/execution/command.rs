//! Unified builder for runner commands.
//!
//! Responsibilities:
//! - Build `Command` instances with runner-specific arguments and temp resources.
//! - Manage temporary prompt files and stdin payloads for runners.
//!
//! Does not handle:
//! - Executing commands or streaming output (see process module).
//! - Validating high-level runner configuration (handled by caller).
//!
//! Assumptions/invariants:
//! - Callers keep temp guards alive until command execution completes.
//! - Provided binaries and working directories are valid and accessible.

use anyhow::{anyhow, Result};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::fsutil;

use super::super::{
    ClaudePermissionMode, Model, ReasoningEffort, OPENCODE_PROMPT_FILE_MESSAGE, TEMP_RETENTION,
};
use super::process::ensure_self_on_path;

/// Builds `std::process::Command` instances with standardized configuration for runners.
#[allow(dead_code)]
pub struct RunnerCommandBuilder {
    cmd: Command,
    bin: String,
    work_dir: PathBuf,
    stdin_payload: Option<Vec<u8>>,
    // We hold these to ensure temp files/dirs persist until the command is built and executed.
    // The caller receives these and must drop them only after execution completes.
    temp_resources: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

impl RunnerCommandBuilder {
    pub fn new(bin: &str, work_dir: &Path) -> Self {
        let mut cmd = Command::new(bin);
        cmd.current_dir(work_dir);
        ensure_self_on_path(&mut cmd);

        Self {
            cmd,
            bin: bin.to_string(),
            work_dir: work_dir.to_path_buf(),
            stdin_payload: None,
            temp_resources: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.cmd.arg(arg);
        self
    }

    #[allow(dead_code)]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.cmd.args(args);
        self
    }

    #[allow(dead_code)]
    pub fn env(mut self, key: &str, val: &str) -> Self {
        self.cmd.env(key, val);
        self
    }

    #[allow(dead_code)]
    pub fn stdout(mut self, cfg: Stdio) -> Self {
        self.cmd.stdout(cfg);
        self
    }

    #[allow(dead_code)]
    pub fn stderr(mut self, cfg: Stdio) -> Self {
        self.cmd.stderr(cfg);
        self
    }

    #[allow(dead_code)]
    pub fn stdin(mut self, cfg: Stdio) -> Self {
        self.cmd.stdin(cfg);
        self
    }

    pub fn model(mut self, model: &Model) -> Self {
        self.cmd.arg("--model").arg(model.as_str());
        self
    }

    pub fn output_format(mut self, format: &str) -> Self {
        self.cmd.arg("--output-format").arg(format);
        self
    }

    pub fn legacy_json_format(mut self) -> Self {
        self.cmd.arg("--json");
        self
    }

    pub fn opencode_format(mut self) -> Self {
        self.cmd.arg("--format").arg("json");
        self
    }

    pub fn reasoning_effort(mut self, effort: Option<ReasoningEffort>) -> Self {
        if let Some(effort) = effort {
            self.cmd.arg("-c").arg(format!(
                "model_reasoning_effort=\"{}\"",
                effort_as_str(effort)
            ));
        }
        self
    }

    pub fn permission_mode(mut self, mode: Option<ClaudePermissionMode>) -> Self {
        let mode = mode.unwrap_or(ClaudePermissionMode::BypassPermissions);
        self.cmd
            .arg("--permission-mode")
            .arg(permission_mode_to_arg(mode));
        self
    }

    pub fn with_temp_prompt_file(mut self, content: &str) -> Result<Self> {
        if let Err(err) = fsutil::cleanup_default_temp_dirs(TEMP_RETENTION) {
            log::warn!("temp cleanup failed: {:#}", err);
        }

        let temp_dir = fsutil::create_ralph_temp_dir("prompt")
            .map_err(|e| temp_prompt_file_error(&self.bin, "create_temp_dir", e))?;

        let mut tmp = tempfile::Builder::new()
            .prefix("prompt_")
            .suffix(".md")
            .tempfile_in(temp_dir.path())
            .map_err(|e| temp_prompt_file_error(&self.bin, "create_temp_prompt_file", e))?;

        tmp.write_all(content.as_bytes())
            .map_err(|e| temp_prompt_file_error(&self.bin, "write_prompt_file", e))?;
        tmp.flush()
            .map_err(|e| temp_prompt_file_error(&self.bin, "flush_prompt_file", e))?;

        self.cmd.arg("--file").arg(tmp.path());
        self.cmd.arg("--").arg(OPENCODE_PROMPT_FILE_MESSAGE);

        // We need to keep both the file and the directory alive.
        // If TempDir is dropped, it removes the directory and its contents.
        // If NamedTempFile is dropped, it removes the file.
        // We push both to resources.
        self.temp_resources.push(Box::new(tmp));
        self.temp_resources.push(Box::new(temp_dir));

        Ok(self)
    }

    pub fn stdin_payload(mut self, payload: Option<Vec<u8>>) -> Self {
        self.stdin_payload = payload;
        self
    }

    pub fn build(
        self,
    ) -> (
        Command,
        Option<Vec<u8>>,
        Vec<Box<dyn std::any::Any + Send + Sync>>,
    ) {
        (self.cmd, self.stdin_payload, self.temp_resources)
    }
}

fn temp_prompt_file_error(bin: &str, step: &str, source: impl fmt::Display) -> anyhow::Error {
    anyhow!(
        "Runner prompt file failed (bin={}, step={}): {}. Ensure the temp directory is writable and has available space.",
        bin,
        step,
        source
    )
}

pub(super) fn effort_as_str(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::XHigh => "xhigh",
    }
}

pub(super) fn permission_mode_to_arg(mode: ClaudePermissionMode) -> &'static str {
    match mode {
        ClaudePermissionMode::AcceptEdits => "acceptEdits",
        ClaudePermissionMode::BypassPermissions => "bypassPermissions",
    }
}

#[cfg(test)]
mod tests {
    use super::temp_prompt_file_error;

    #[test]
    fn temp_prompt_file_error_includes_bin_and_step() {
        let err = temp_prompt_file_error("opencode", "create_temp_dir", "boom");
        let msg = format!("{err}");
        assert!(msg.contains("bin=opencode"));
        assert!(msg.contains("step=create_temp_dir"));
        assert!(msg.contains("boom"));
    }
}
