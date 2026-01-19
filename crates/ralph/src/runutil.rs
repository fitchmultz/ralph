//! Shared helpers for runner invocations with consistent error handling.

use crate::contracts::{ClaudePermissionMode, Model, ReasoningEffort, Runner};
use crate::{gitutil, outpututil, runner};
use anyhow::{bail, Result};
use std::path::Path;
use std::time::Duration;

pub struct RunnerInvocation<'a> {
    pub repo_root: &'a Path,
    pub runner_kind: Runner,
    pub bins: runner::RunnerBinaries<'a>,
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub prompt: &'a str,
    pub timeout: Option<Duration>,
    pub two_pass_plan: bool,
    pub permission_mode: Option<ClaudePermissionMode>,
}

pub struct RunnerErrorMessages<'a, FNonZero, FOther>
where
    FNonZero: FnOnce(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    pub log_label: &'a str,
    pub interrupted_msg: &'a str,
    pub timeout_msg: &'a str,
    pub terminated_msg: &'a str,
    pub non_zero_msg: FNonZero,
    pub other_msg: FOther,
}

pub fn run_prompt_with_handling<FNonZero, FOther>(
    invocation: RunnerInvocation<'_>,
    messages: RunnerErrorMessages<'_, FNonZero, FOther>,
) -> Result<runner::RunnerOutput>
where
    FNonZero: FnOnce(i32) -> String,
    FOther: FnOnce(runner::RunnerError) -> String,
{
    let RunnerInvocation {
        repo_root,
        runner_kind,
        bins,
        model,
        reasoning_effort,
        prompt,
        timeout,
        two_pass_plan,
        permission_mode,
    } = invocation;
    let RunnerErrorMessages {
        log_label,
        interrupted_msg,
        timeout_msg,
        terminated_msg,
        non_zero_msg,
        other_msg,
    } = messages;

    match runner::run_prompt(
        runner_kind,
        repo_root,
        bins,
        model,
        reasoning_effort,
        prompt,
        timeout,
        two_pass_plan,
        permission_mode,
    ) {
        Ok(output) => Ok(output),
        Err(runner::RunnerError::Interrupted) => {
            gitutil::revert_uncommitted(repo_root)?;
            bail!("{}", interrupted_msg);
        }
        Err(runner::RunnerError::Timeout) => {
            bail!("{}", timeout_msg);
        }
        Err(runner::RunnerError::NonZeroExit { code, stderr, .. }) => {
            log_stderr_tail(log_label, &stderr.to_string());
            gitutil::revert_uncommitted(repo_root)?;
            bail!("{}", non_zero_msg(code));
        }
        Err(runner::RunnerError::TerminatedBySignal { stderr, .. }) => {
            log_stderr_tail(log_label, &stderr.to_string());
            gitutil::revert_uncommitted(repo_root)?;
            bail!("{}", terminated_msg);
        }
        Err(err) => {
            gitutil::revert_uncommitted(repo_root)?;
            bail!("{}", other_msg(err));
        }
    }
}

fn log_stderr_tail(label: &str, stderr: &str) {
    let tail = outpututil::tail_lines(
        stderr,
        outpututil::OUTPUT_TAIL_LINES,
        outpututil::OUTPUT_TAIL_LINE_MAX_CHARS,
    );
    if tail.is_empty() {
        return;
    }

    log::error!("{label} stderr (tail):");
    for line in tail {
        log::info!("{label}: {line}");
    }
}
