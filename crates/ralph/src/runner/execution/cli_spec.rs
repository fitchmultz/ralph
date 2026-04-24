//! Runner-specific CLI flag mapping for normalized options.
//!
//! Purpose:
//! - Runner-specific CLI flag mapping for normalized options.
//!
//! Responsibilities:
//! - Translate normalized runner CLI options into runner-specific CLI flags.
//! - Preserve required ordering constraints (e.g., Codex global options before `exec`).
//!
//! Non-scope:
//! - Resolving option precedence (see `cli_options`).
//! - Prompt rendering or process execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Output format is validated upstream to be newline-delimited JSON (`stream_json`).
//! - Unsupported options are validated upstream; mapping generally performs no-op for them.
//!
//! IMPORTANT - Codex Approval Mode Behavior:
//! Ralph intentionally does NOT pass any approval flags (`-a`, `--ask-for-approval`) to Codex.
//! This allows Codex to use the user's global config file (`~/.codex/config.json`) settings.
//! If you are tempted to add approval flag support for Codex here, DON'T. This is by design.
//! The only exception is when sandbox is disabled, which passes `--dangerously-bypass-approvals-and-sandbox`.

use crate::commands::run::PhaseType;
use crate::contracts::{RunnerApprovalMode, RunnerPlanMode, RunnerSandboxMode, RunnerVerbosity};

use super::cli_options::ResolvedRunnerCliOptions;
use super::command::RunnerCommandBuilder;

pub(super) fn apply_codex_global_options(
    builder: RunnerCommandBuilder,
    opts: ResolvedRunnerCliOptions,
) -> RunnerCommandBuilder {
    if opts.sandbox == RunnerSandboxMode::Disabled {
        return builder.arg("--dangerously-bypass-approvals-and-sandbox");
    }

    // NOTE: We intentionally do NOT pass any approval flags to codex.
    // This allows codex to use the user's global config file settings.
    // Users can set their preferred approval mode in ~/.codex/config.json
    // and ralph will honor it without overriding via CLI flags.

    let sandbox_value = match opts.sandbox {
        RunnerSandboxMode::Enabled => Some("workspace-write"),
        RunnerSandboxMode::Default => None,
        RunnerSandboxMode::Disabled => None,
    };

    builder.arg_opt("--sandbox", sandbox_value)
}

pub(super) fn apply_gemini_options(
    builder: RunnerCommandBuilder,
    opts: ResolvedRunnerCliOptions,
) -> RunnerCommandBuilder {
    let builder = match opts.approval_mode {
        RunnerApprovalMode::Yolo => builder.args(["--approval-mode", "yolo"]),
        RunnerApprovalMode::AutoEdits => builder.args(["--approval-mode", "auto_edit"]),
        RunnerApprovalMode::Default | RunnerApprovalMode::Safe => builder,
    };

    match opts.sandbox {
        RunnerSandboxMode::Enabled => builder.arg("--sandbox"),
        RunnerSandboxMode::Disabled | RunnerSandboxMode::Default => builder,
    }
}

pub(super) fn apply_claude_options(
    builder: RunnerCommandBuilder,
    opts: ResolvedRunnerCliOptions,
) -> RunnerCommandBuilder {
    match opts.verbosity {
        RunnerVerbosity::Verbose => builder.arg("--verbose"),
        RunnerVerbosity::Quiet | RunnerVerbosity::Normal => builder,
    }
}

pub(super) fn apply_cursor_options(
    mut builder: RunnerCommandBuilder,
    opts: ResolvedRunnerCliOptions,
    phase_type: PhaseType,
) -> RunnerCommandBuilder {
    let is_planning = phase_type == PhaseType::Planning;

    if opts.approval_mode == RunnerApprovalMode::Yolo {
        builder = builder.arg("--force");
    }

    let sandbox_mode = match opts.sandbox {
        RunnerSandboxMode::Enabled => "enabled",
        RunnerSandboxMode::Disabled => "disabled",
        RunnerSandboxMode::Default => {
            if is_planning {
                "enabled"
            } else {
                "disabled"
            }
        }
    };
    builder = builder.args(["--sandbox", sandbox_mode]);

    let plan_enabled = match opts.plan_mode {
        RunnerPlanMode::Enabled => true,
        RunnerPlanMode::Disabled => false,
        RunnerPlanMode::Default => is_planning,
    };
    if plan_enabled {
        builder = builder.arg("--plan");
    }

    builder
}

pub(super) fn apply_kimi_options(
    builder: RunnerCommandBuilder,
    opts: ResolvedRunnerCliOptions,
) -> RunnerCommandBuilder {
    // Kimi uses --yolo or -y for auto-approval, NOT --approval-mode yolo
    match opts.approval_mode {
        RunnerApprovalMode::Yolo => builder.arg("--yolo"),
        RunnerApprovalMode::AutoEdits => builder.arg("--yolo"), // Kimi doesn't have auto-edits mode, use yolo
        RunnerApprovalMode::Default | RunnerApprovalMode::Safe => builder,
    }
}

pub(super) fn apply_pi_options(
    builder: RunnerCommandBuilder,
    opts: ResolvedRunnerCliOptions,
) -> RunnerCommandBuilder {
    // Pi uses --print (-p) for non-interactive/auto-approve mode, not --approval-mode
    let builder = match opts.approval_mode {
        RunnerApprovalMode::Yolo => builder.arg("--print"),
        RunnerApprovalMode::AutoEdits => builder.arg("--print"), // Pi doesn't have auto-edits mode, use print
        RunnerApprovalMode::Default | RunnerApprovalMode::Safe => builder,
    };

    match opts.sandbox {
        RunnerSandboxMode::Enabled => builder.arg("--sandbox"),
        RunnerSandboxMode::Disabled | RunnerSandboxMode::Default => builder,
    }
}

#[cfg(test)]
mod tests {
    use super::super::cli_options::ResolvedRunnerCliOptions;
    use super::super::command::RunnerCommandBuilder;
    use super::apply_codex_global_options;
    use crate::contracts::{RunnerApprovalMode, RunnerSandboxMode};
    use std::path::Path;

    #[test]
    fn codex_sandbox_disabled_uses_bypass_flag() {
        let opts = ResolvedRunnerCliOptions {
            approval_mode: RunnerApprovalMode::Yolo,
            sandbox: RunnerSandboxMode::Disabled,
            ..ResolvedRunnerCliOptions::default()
        };

        let (cmd, _payload, _guards) =
            apply_codex_global_options(RunnerCommandBuilder::new("codex", Path::new(".")), opts)
                .build();

        let args = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(args, vec!["--dangerously-bypass-approvals-and-sandbox"]);
    }

    #[test]
    fn codex_yolo_sets_no_approval_flags() {
        // Yolo mode should NOT pass -a never to codex; we defer to user's global codex config
        let opts = ResolvedRunnerCliOptions {
            approval_mode: RunnerApprovalMode::Yolo,
            sandbox: RunnerSandboxMode::Default,
            ..ResolvedRunnerCliOptions::default()
        };

        let (cmd, _payload, _guards) =
            apply_codex_global_options(RunnerCommandBuilder::new("codex", Path::new(".")), opts)
                .build();

        let args = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(args.is_empty());
    }

    #[test]
    fn codex_sandbox_enabled_sets_workspace_write() {
        let opts = ResolvedRunnerCliOptions {
            approval_mode: RunnerApprovalMode::Yolo,
            sandbox: RunnerSandboxMode::Enabled,
            ..ResolvedRunnerCliOptions::default()
        };

        let (cmd, _payload, _guards) =
            apply_codex_global_options(RunnerCommandBuilder::new("codex", Path::new(".")), opts)
                .build();

        let args = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        // No -a never flag passed; we defer to user's global codex config
        assert_eq!(args, vec!["--sandbox", "workspace-write"]);
    }
}
