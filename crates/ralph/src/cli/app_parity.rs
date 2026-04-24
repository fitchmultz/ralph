//! RalphMac CLI parity registry.
//!
//! Purpose:
//! - RalphMac CLI parity registry.
//!
//! Responsibilities:
//! - Classify human-facing top-level CLI command families for native app parity work.
//! - Keep owner/work-zone, machine-contract, app-surface, and test expectations together.
//! - Provide coverage helpers so new top-level CLI commands cannot appear unclassified.
//!
//! Not handled here:
//! - Hidden machine subcommands.
//! - Runtime dispatch or machine JSON contract execution.
//! - Per-subcommand implementation status beyond the owner feature family.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Every non-machine top-level command has exactly one registry entry.
//! - Advanced Runner access never counts as parity-complete by itself.
//! - Registry status is an implementation tracker, not a human help surface.

use clap::CommandFactory;

use super::Cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppParityStatus {
    NativeReady,
    InProgress,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppParityEntry {
    pub command: &'static str,
    pub status: AppParityStatus,
    pub owner_feature: &'static str,
    pub machine_contract: &'static str,
    pub app_surface: &'static str,
    pub tests: &'static str,
}

pub const APP_PARITY_REGISTRY: &[AppParityEntry] = &[
    entry(
        "app",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "app status contract",
        "Install/update status",
        "App status UI + contract tests",
    ),
    entry(
        "cleanup",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine cleanup",
        "Cleanup panel",
        "Cleanup preview/write tests",
    ),
    entry(
        "cli-spec",
        AppParityStatus::NativeReady,
        "Diagnostics",
        "machine cli-spec",
        "Advanced Runner diagnostics",
        "CLI spec decode tests",
    ),
    entry(
        "completions",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine completions",
        "Completions panel",
        "Shell output tests",
    ),
    entry(
        "config",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine config",
        "Configuration editor",
        "Config decode/save tests",
    ),
    entry(
        "context",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine context",
        "Context panel",
        "Context preview/write tests",
    ),
    entry(
        "daemon",
        AppParityStatus::InProgress,
        "Automation",
        "machine daemon",
        "Daemon panel",
        "Daemon status/action tests",
    ),
    entry(
        "doctor",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine doctor",
        "Doctor panel",
        "Doctor report tests",
    ),
    entry(
        "help-all",
        AppParityStatus::Blocked,
        "Workspace Admin",
        "not needed",
        "Command reference",
        "Reference snapshot tests",
    ),
    entry(
        "init",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine init",
        "Workspace init",
        "Init temp-workspace tests",
    ),
    entry(
        "migrate",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine migrate",
        "Migration panel",
        "Migration dry-run/apply tests",
    ),
    entry(
        "plugin",
        AppParityStatus::InProgress,
        "Automation",
        "machine plugin",
        "Plugin manager",
        "Plugin list/install tests",
    ),
    entry(
        "prd",
        AppParityStatus::InProgress,
        "Create & Discover",
        "machine prd",
        "PRD import",
        "PRD preview/write tests",
    ),
    entry(
        "productivity",
        AppParityStatus::InProgress,
        "Analytics",
        "machine productivity",
        "Analytics",
        "Productivity decode tests",
    ),
    entry(
        "prompt",
        AppParityStatus::InProgress,
        "Automation",
        "machine prompt",
        "Prompt preview",
        "Prompt preview tests",
    ),
    entry(
        "queue",
        AppParityStatus::InProgress,
        "Queue Ops",
        "machine queue",
        "Queue operations",
        "Queue workflow tests",
    ),
    entry(
        "run",
        AppParityStatus::InProgress,
        "Run Control",
        "machine run",
        "Run Control",
        "Run control UI + contract tests",
    ),
    entry(
        "runner",
        AppParityStatus::InProgress,
        "Run Control",
        "machine runner",
        "Runner settings",
        "Runner capabilities tests",
    ),
    entry(
        "scan",
        AppParityStatus::InProgress,
        "Create & Discover",
        "machine scan",
        "Scan panel",
        "Scan preview/write tests",
    ),
    entry(
        "task",
        AppParityStatus::InProgress,
        "Work Queue/Create & Discover",
        "machine task",
        "Task workflows",
        "Task creation/mutation tests",
    ),
    entry(
        "tutorial",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine tutorial",
        "Tutorial panel",
        "Tutorial smoke tests",
    ),
    entry(
        "undo",
        AppParityStatus::InProgress,
        "Queue Ops",
        "machine queue undo",
        "Recovery history",
        "Undo preview/restore tests",
    ),
    entry(
        "version",
        AppParityStatus::InProgress,
        "Workspace Admin",
        "machine system info",
        "Version panel",
        "Version/status tests",
    ),
    entry(
        "watch",
        AppParityStatus::InProgress,
        "Automation",
        "machine watch",
        "Watch panel",
        "Watch workflow tests",
    ),
    entry(
        "webhook",
        AppParityStatus::InProgress,
        "Automation",
        "machine webhook",
        "Webhook panel",
        "Webhook status/replay tests",
    ),
];

const fn entry(
    command: &'static str,
    status: AppParityStatus,
    owner_feature: &'static str,
    machine_contract: &'static str,
    app_surface: &'static str,
    tests: &'static str,
) -> AppParityEntry {
    AppParityEntry {
        command,
        status,
        owner_feature,
        machine_contract,
        app_surface,
        tests,
    }
}

pub fn unclassified_human_cli_commands() -> Vec<String> {
    let registered = APP_PARITY_REGISTRY
        .iter()
        .map(|entry| entry.command)
        .collect::<std::collections::BTreeSet<_>>();

    Cli::command()
        .get_subcommands()
        .filter(|command| !command.is_hide_set())
        .map(clap::Command::get_name)
        .chain(
            Cli::command()
                .get_subcommands()
                .filter(|command| command.is_hide_set() && command.get_name() != "machine")
                .map(clap::Command::get_name),
        )
        .filter(|name| !registered.contains(name))
        .map(str::to_string)
        .collect()
}
