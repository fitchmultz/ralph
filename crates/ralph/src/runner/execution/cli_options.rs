//! Normalized runner CLI option resolution.
//!
//! Responsibilities:
//! - Resolve normalized runner CLI options from (CLI overrides -> task -> config).
//! - Provide runner-aware derivations needed by command assembly (e.g., Claude permission mode).
//!
//! Does not handle:
//! - Building runner `Command` arguments (see `cli_spec` / `command` / `runners`).
//! - Executing runner processes or parsing output (see `process` / `stream`).
//!
//! Invariants/assumptions:
//! - Ralph execution requires newline-delimited JSON objects; non-stream formats are rejected.
//! - Defaults are intentionally permissive (YOLO) unless overridden.

use anyhow::{Result, bail};

use crate::contracts::{
    AgentConfig, ClaudePermissionMode, Runner, RunnerApprovalMode, RunnerCliOptionsPatch,
    RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity,
    UnsupportedOptionPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ResolvedRunnerCliOptions {
    pub(crate) output_format: RunnerOutputFormat,
    pub(crate) verbosity: RunnerVerbosity,
    pub(crate) approval_mode: RunnerApprovalMode,
    pub(crate) sandbox: RunnerSandboxMode,
    pub(crate) plan_mode: RunnerPlanMode,
    pub(crate) unsupported_option_policy: UnsupportedOptionPolicy,
}

impl Default for ResolvedRunnerCliOptions {
    fn default() -> Self {
        Self {
            output_format: RunnerOutputFormat::StreamJson,
            verbosity: RunnerVerbosity::Normal,
            approval_mode: RunnerApprovalMode::Yolo,
            sandbox: RunnerSandboxMode::Default,
            plan_mode: RunnerPlanMode::Default,
            unsupported_option_policy: UnsupportedOptionPolicy::Warn,
        }
    }
}

fn merged_patch_for_runner(
    runner: &Runner,
    cli_patch: &RunnerCliOptionsPatch,
    task_patch: Option<&RunnerCliOptionsPatch>,
    agent: &AgentConfig,
) -> RunnerCliOptionsPatch {
    let mut merged = RunnerCliOptionsPatch::default();

    if let Some(root) = agent.runner_cli.as_ref() {
        merged.merge_from(root.defaults.clone());
        if let Some(patch) = root.runners.get(runner) {
            merged.merge_from(patch.clone());
        }
    }

    if let Some(patch) = task_patch {
        merged.merge_from(patch.clone());
    }

    merged.merge_from(cli_patch.clone());

    merged
}

pub(crate) fn resolve_runner_cli_options(
    runner: &Runner,
    cli_patch: &RunnerCliOptionsPatch,
    task_patch: Option<&RunnerCliOptionsPatch>,
    agent: &AgentConfig,
) -> Result<ResolvedRunnerCliOptions> {
    let patch = merged_patch_for_runner(runner, cli_patch, task_patch, agent);
    let mut resolved = ResolvedRunnerCliOptions::default();

    if let Some(value) = patch.output_format {
        resolved.output_format = value;
    }
    if let Some(value) = patch.verbosity {
        resolved.verbosity = value;
    }
    if let Some(value) = patch.approval_mode {
        resolved.approval_mode = value;
    }
    if let Some(value) = patch.sandbox {
        resolved.sandbox = value;
    }
    if let Some(value) = patch.plan_mode {
        resolved.plan_mode = value;
    }
    if let Some(value) = patch.unsupported_option_policy {
        resolved.unsupported_option_policy = value;
    }

    resolved.validate_for_execution(runner)?;
    Ok(resolved)
}

impl ResolvedRunnerCliOptions {
    pub(crate) fn validate_for_execution(self, runner: &Runner) -> Result<()> {
        if self.output_format != RunnerOutputFormat::StreamJson {
            bail!(
                "runner_cli.output_format={:?} is not supported for execution. Ralph requires newline-delimited JSON objects; set runner_cli.output_format=stream_json.",
                self.output_format
            );
        }

        if self.plan_mode != RunnerPlanMode::Default && runner != &Runner::Cursor {
            self.unsupported("plan_mode", runner)?;
        }
        if self.verbosity == RunnerVerbosity::Verbose && runner != &Runner::Claude {
            self.unsupported("verbosity=verbose", runner)?;
        }
        if self.sandbox != RunnerSandboxMode::Default
            && !matches!(
                runner,
                Runner::Codex | Runner::Gemini | Runner::Cursor | Runner::Plugin(_)
            )
        {
            self.unsupported("sandbox", runner)?;
        }
        if self.approval_mode == RunnerApprovalMode::AutoEdits
            && !matches!(runner, Runner::Gemini | Runner::Claude | Runner::Plugin(_))
        {
            self.unsupported("approval_mode=auto_edits", runner)?;
        }
        if self.approval_mode == RunnerApprovalMode::Safe {
            // Safe mode is currently not implemented consistently across runners and may
            // cause interactive prompts/hangs.
            self.unsupported("approval_mode=safe", runner)?;
        }

        Ok(())
    }

    pub(crate) fn effective_claude_permission_mode(
        self,
        legacy: Option<ClaudePermissionMode>,
    ) -> Option<ClaudePermissionMode> {
        match self.approval_mode {
            RunnerApprovalMode::AutoEdits => Some(ClaudePermissionMode::AcceptEdits),
            RunnerApprovalMode::Yolo => Some(ClaudePermissionMode::BypassPermissions),
            RunnerApprovalMode::Default | RunnerApprovalMode::Safe => legacy,
        }
    }

    fn unsupported(self, setting: &str, runner: &Runner) -> Result<()> {
        match self.unsupported_option_policy {
            UnsupportedOptionPolicy::Ignore => Ok(()),
            UnsupportedOptionPolicy::Warn => {
                log::warn!(
                    "runner_cli: requested {setting} for runner {:?}, but it is not supported; ignoring",
                    runner
                );
                Ok(())
            }
            UnsupportedOptionPolicy::Error => bail!(
                "runner_cli: requested {setting} for runner {:?}, but it is not supported (set unsupported_option_policy=warn to ignore)",
                runner
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{RunnerCliConfigRoot, RunnerCliOptionsPatch};
    use std::collections::BTreeMap;

    #[test]
    fn resolve_runner_cli_options_rejects_non_stream_output() {
        let agent = AgentConfig {
            runner_cli: Some(RunnerCliConfigRoot {
                defaults: RunnerCliOptionsPatch {
                    output_format: Some(RunnerOutputFormat::Text),
                    ..Default::default()
                },
                runners: BTreeMap::new(),
            }),
            ..Default::default()
        };

        let err = resolve_runner_cli_options(
            &Runner::Codex,
            &RunnerCliOptionsPatch::default(),
            None,
            &agent,
        )
        .expect_err("expected error");
        assert!(err.to_string().contains("output_format"));
        assert!(err.to_string().contains("stream_json"));
    }

    #[test]
    fn unsupported_option_policy_error_fails_fast() {
        let agent = AgentConfig {
            runner_cli: Some(RunnerCliConfigRoot {
                defaults: RunnerCliOptionsPatch {
                    unsupported_option_policy: Some(UnsupportedOptionPolicy::Error),
                    plan_mode: Some(RunnerPlanMode::Enabled),
                    ..Default::default()
                },
                runners: BTreeMap::new(),
            }),
            ..Default::default()
        };

        let err = resolve_runner_cli_options(
            &Runner::Claude,
            &RunnerCliOptionsPatch::default(),
            None,
            &agent,
        )
        .expect_err("expected error");
        assert!(err.to_string().contains("plan_mode"));
        assert!(err.to_string().contains("not supported"));
    }

    #[test]
    fn unsupported_option_policy_warn_does_not_fail() -> Result<()> {
        let agent = AgentConfig {
            runner_cli: Some(RunnerCliConfigRoot {
                defaults: RunnerCliOptionsPatch {
                    unsupported_option_policy: Some(UnsupportedOptionPolicy::Warn),
                    plan_mode: Some(RunnerPlanMode::Enabled),
                    ..Default::default()
                },
                runners: BTreeMap::new(),
            }),
            ..Default::default()
        };

        let resolved = resolve_runner_cli_options(
            &Runner::Codex,
            &RunnerCliOptionsPatch::default(),
            None,
            &agent,
        )?;
        assert_eq!(resolved.plan_mode, RunnerPlanMode::Enabled);
        Ok(())
    }
}
