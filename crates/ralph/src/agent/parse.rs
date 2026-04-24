//! Parsing functions for agent-related CLI inputs.
//!
//! Purpose:
//! - Parsing functions for agent-related CLI inputs.
//!
//! Responsibilities:
//! - Parse runner strings into Runner enum variants.
//! - Parse git revert mode strings into GitRevertMode enum.
//! - Parse runner CLI arguments into RunnerCliOptionsPatch structs.
//!
//! Not handled here:
//! - Model parsing (see `crate::runner`).
//! - Reasoning effort parsing (see `crate::runner`).
//! - Override resolution (see `super::resolve`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Parsing is case-insensitive for runner strings.
//! - Invalid inputs return descriptive errors via anyhow.

use crate::contracts::{
    GitPublishMode, GitRevertMode, Runner, RunnerApprovalMode, RunnerCliOptionsPatch,
    RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
    UnsupportedOptionPolicy,
};
use anyhow::{Result, anyhow, bail};

use super::args::RunnerCliArgs;

/// Parse a runner string into a Runner enum.
pub fn parse_runner(value: &str) -> Result<Runner> {
    let normalized = value.trim().to_lowercase();
    match normalized.as_str() {
        "codex" => Ok(Runner::Codex),
        "opencode" => Ok(Runner::Opencode),
        "gemini" => Ok(Runner::Gemini),
        "claude" => Ok(Runner::Claude),
        "cursor" => Ok(Runner::Cursor),
        "kimi" => Ok(Runner::Kimi),
        "pi" => Ok(Runner::Pi),
        _ => bail!(
            "Invalid runner: --runner must be 'codex', 'opencode', 'gemini', 'claude', 'cursor', 'kimi', or 'pi' (got: {}). Set a supported runner in .ralph/config.jsonc or via the --runner flag.",
            value.trim()
        ),
    }
}

/// Parse git revert mode from a CLI string.
pub fn parse_git_revert_mode(value: &str) -> Result<GitRevertMode> {
    value.parse().map_err(|err: &str| anyhow!(err))
}

/// Parse git publish mode from a CLI string.
pub fn parse_git_publish_mode(value: &str) -> Result<GitPublishMode> {
    value.parse().map_err(|err: &str| anyhow!(err))
}

/// Parse runner CLI arguments into a patch struct.
pub(crate) fn parse_runner_cli_patch(args: &RunnerCliArgs) -> Result<RunnerCliOptionsPatch> {
    let output_format = match args.output_format.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerOutputFormat>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let verbosity = match args.verbosity.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerVerbosity>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let approval_mode = match args.approval_mode.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerApprovalMode>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let sandbox = match args.sandbox.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerSandboxMode>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let plan_mode = match args.plan_mode.as_deref() {
        Some(value) => Some(
            value
                .parse::<RunnerPlanMode>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };
    let unsupported_option_policy = match args.unsupported_option_policy.as_deref() {
        Some(value) => Some(
            value
                .parse::<UnsupportedOptionPolicy>()
                .map_err(|err| anyhow!(err))?,
        ),
        None => None,
    };

    Ok(RunnerCliOptionsPatch {
        output_format,
        verbosity,
        approval_mode,
        sandbox,
        plan_mode,
        unsupported_option_policy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runner_accepts_valid_runners() {
        assert!(matches!(parse_runner("codex"), Ok(Runner::Codex)));
        assert!(matches!(parse_runner("opencode"), Ok(Runner::Opencode)));
        assert!(matches!(parse_runner("gemini"), Ok(Runner::Gemini)));
        assert!(matches!(parse_runner("claude"), Ok(Runner::Claude)));
        assert!(matches!(parse_runner("cursor"), Ok(Runner::Cursor)));
        assert!(matches!(parse_runner("kimi"), Ok(Runner::Kimi)));
        assert!(matches!(parse_runner("pi"), Ok(Runner::Pi)));
        assert!(matches!(parse_runner("CODEX"), Ok(Runner::Codex)));
    }

    #[test]
    fn parse_runner_rejects_invalid_runners() {
        assert!(parse_runner("invalid").is_err());
        assert!(parse_runner("").is_err());
    }
}
