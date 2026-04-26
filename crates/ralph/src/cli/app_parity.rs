//! RalphMac app parity registry.
//!
//! Purpose:
//! - Track user-visible CLI versus RalphMac parity scenarios with concrete proof anchors.
//!
//! Responsibilities:
//! - Keep scenario-level parity status, contract anchors, app surfaces, and proof tests together.
//! - Validate that every parity scenario names machine/app contract anchors plus Rust and RalphMac tests.
//! - Preserve a secondary guard that every human-facing root CLI command is classified for parity review.
//!
//! Not handled here:
//! - Runtime dispatch or machine JSON contract execution.
//! - App-specific implementation details beyond their parity surface names.
//! - Human-readable release notes or roadmap planning.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Scenario-level entries are the authoritative parity signal for RalphMac.
//! - Every scenario must include machine-contract anchors, app-doc anchors, Rust proofs, and RalphMac proofs.
//! - Advanced Runner access never counts as parity completion by itself.

use clap::CommandFactory;

use super::Cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppParityScenarioStatus {
    NativeReady,
    InProgress,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppParityScenarioEntry {
    pub scenario: &'static str,
    pub status: AppParityScenarioStatus,
    pub owner_feature: &'static str,
    pub machine_contract: &'static [&'static str],
    pub app_contract: &'static [&'static str],
    pub app_surface: &'static str,
    pub rust_tests: &'static [&'static str],
    pub app_tests: &'static [&'static str],
    pub notes: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppParityCoverageIssue {
    pub scenario: &'static str,
    pub problem: &'static str,
}

macro_rules! scenario {
    (
        $scenario:expr,
        $status:expr,
        $owner_feature:expr,
        $machine_contract:expr,
        $app_contract:expr,
        $app_surface:expr,
        $rust_tests:expr,
        $app_tests:expr,
        $notes:expr $(,)?
    ) => {
        AppParityScenarioEntry {
            scenario: $scenario,
            status: $status,
            owner_feature: $owner_feature,
            machine_contract: $machine_contract,
            app_contract: $app_contract,
            app_surface: $app_surface,
            rust_tests: $rust_tests,
            app_tests: $app_tests,
            notes: $notes,
        }
    };
}

pub const APP_PARITY_COMMAND_GUARD: &[&str] = &[
    "app",
    "cleanup",
    "cli-spec",
    "completions",
    "config",
    "context",
    "daemon",
    "doctor",
    "help-all",
    "init",
    "migrate",
    "plugin",
    "prd",
    "productivity",
    "prompt",
    "queue",
    "run",
    "runner",
    "scan",
    "task",
    "tutorial",
    "undo",
    "version",
    "watch",
    "webhook",
];

pub const APP_PARITY_SCENARIO_REGISTRY: &[AppParityScenarioEntry] = &[
    scenario!(
        "run_loop_empty_queue_summary",
        AppParityScenarioStatus::NativeReady,
        "Run Control",
        &[
            "machine run loop summary",
            "BlockingState idle/no_candidates summary",
        ],
        &[
            "docs/features/app.md: native Run Control uses machine-backed contracts",
            "docs/features/app.md: CLI and app should remain behaviorally aligned for core task/run operations",
        ],
        "Run Control idle/no-work summary",
        &[
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_loop_empty_repo_reports_no_candidates_summary",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_loop_parallel_empty_repo_reports_no_candidates_summary",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/WorkspaceRunStateResumeBlockingTests.swift::test_runSummary_explicitBlocking_supersedesEarlierLiveBlockingState",
        ],
        &[],
    ),
    scenario!(
        "run_loop_blocked_queue_summary",
        AppParityScenarioStatus::NativeReady,
        "Run Control",
        &[
            "machine run loop summary",
            "BlockingState schedule_blocked summary",
        ],
        &[
            "docs/features/app.md: native Run Control uses machine-backed contracts",
            "docs/features/app.md: CLI and app should remain behaviorally aligned for core task/run operations",
        ],
        "Run Control blocked/scheduled summary",
        &[
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_loop_dependency_blocked_repo_reports_blocked_summary",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_loop_parallel_blocked_repo_reports_blocked_summary",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/WorkspaceRunStateResumeBlockingTests.swift::test_runSummary_appliesBlockingState",
        ],
        &[],
    ),
    scenario!(
        "run_loop_failure_after_run_started",
        AppParityScenarioStatus::NativeReady,
        "Run Control",
        &[
            "machine run loop run_started event",
            "machine run loop terminal summary",
            "machine_error stderr contract",
        ],
        &[
            "docs/features/app.md: native Run Control uses machine-backed contracts",
            "docs/features/app.md: CLI and app should remain behaviorally aligned for core task/run operations",
        ],
        "Run Control failed/stalled terminal handling",
        &[
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_loop_runtime_failure_still_emits_terminal_summary",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_loop_queue_lock_failure_emits_stalled_terminal_summary",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/WorkspaceRunStateResumeBlockingTests.swift::test_runSummary_failedOutcomeClearsExistingLiveBlockingState",
        ],
        &["Failure after run_started still requires a terminal summary the app can reconcile."],
    ),
    scenario!(
        "run_stop_after_current_machine_contract",
        AppParityScenarioStatus::NativeReady,
        "Run Control",
        &["machine run stop", "continuation.next_steps"],
        &[
            "docs/features/app.md: Stop After Current specifically uses ralph machine run stop",
            "docs/features/app.md: Run Control continuation cards should prefer structured native actions",
            "docs/machine-contract.md: machine run stop (version: 1)",
        ],
        "Run Control Stop After Current",
        &[
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_stop_creates_stop_marker_document",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_stop_reports_already_present_marker",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_stop_dry_run_previews_marker_without_writing",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_stop_uses_runtime_parallel_state_for_guidance",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/WorkspaceLoopStopTests.swift::test_stopLoop_requestsQueueStopSignalForActiveMachineLoop",
            "apps/RalphMac/RalphCoreTests/WorkspaceLoopStopTests.swift::test_stopLoop_keepsStopRequestedWhenMachineRunStopReportsAlreadyPresent",
            "apps/RalphMac/RalphCoreTests/WorkspaceLoopStopTests.swift::test_runControlOperatorState_exposesStopAfterCurrentActionDuringLoopBlocking",
        ],
        &["Human queue stop parsing does not count as parity completion."],
    ),
    scenario!(
        "workspace_custom_queue_path_resolution",
        AppParityScenarioStatus::NativeReady,
        "Workspace Admin",
        &["machine config resolve", "machine workspace overview"],
        &[
            "docs/features/app.md: native workflows should use versioned ralph machine contracts",
            "docs/features/app.md: most data and execution issues can be reproduced via CLI commands",
        ],
        "Workspace bootstrap and diagnostics path resolution",
        &[
            "crates/ralph/tests/config_test/repo_paths.rs::test_resolve_queue_path_custom_relative",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_queue.rs::machine_workspace_overview_returns_queue_and_config_in_one_document",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/WorkspaceDiagnosticsServiceTests.swift::testQueueValidationOutput_reportsConfiguredQueuePathWhenCustomQueueIsMissing",
            "apps/RalphMac/RalphCoreTests/WorkspaceRunnerRetargetingTests.swift::test_workspaceBootstrap_loadsTasksAndRunnerConfigurationWithoutGraphAnalyticsOrCLISpec",
        ],
        &[],
    ),
    scenario!(
        "execution_controls_plugin_runner_visibility",
        AppParityScenarioStatus::NativeReady,
        "Run Control",
        &["machine config resolve.execution_controls.runners"],
        &[
            "docs/features/app.md: execution affordances should come from ralph machine config resolve.execution_controls",
            "docs/features/app.md: trusted plugin runners appear in native controls through the same machine-fed contract",
        ],
        "Runner settings and native execution controls",
        &[
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_one_without_id_reports_selected_task_via_events_and_summary",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/ConfigModelsTests.swift::test_decode_machineConfigResolve_includesWebhookUrlPolicyFields",
        ],
        &["Unknown configured runner values must remain visible instead of being coerced away."],
    ),
    scenario!(
        "execution_controls_parallel_workers_above_menu_default",
        AppParityScenarioStatus::NativeReady,
        "Run Control",
        &[
            "machine config resolve.execution_controls.parallel_workers",
            "machine workspace overview config payload",
        ],
        &[
            "docs/features/app.md: execution affordances should come from ralph machine config resolve.execution_controls",
            "docs/features/app.md: unknown configured runner or effort values must remain visible instead of being coerced away",
        ],
        "Parallel worker controls above legacy menu defaults",
        &[
            "crates/ralph/tests/machine_contract_test/machine_contract_test_run.rs::machine_run_one_without_id_reports_selected_task_via_events_and_summary",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_queue.rs::machine_workspace_overview_returns_queue_and_config_in_one_document",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/ConfigModelsTests.swift::test_decode_machineConfigResolve_includesWebhookUrlPolicyFields",
        ],
        &[
            "The app must preserve machine-reported maxima such as 255 rather than truncating to old menu caps.",
        ],
    ),
    scenario!(
        "continuation_next_steps_native_actions",
        AppParityScenarioStatus::NativeReady,
        "Run Control",
        &[
            "machine run parallel-status",
            "machine queue validate",
            "continuation.next_steps",
        ],
        &[
            "docs/features/app.md: Run Control continuation cards should prefer structured native actions",
            "docs/features/app.md: queue recovery remains preview-first in the app",
            "docs/machine-contract.md: machine run parallel-status (version: 3)",
        ],
        "Continuation cards and operator action mapping",
        &[
            "crates/ralph/tests/machine_contract_test/machine_contract_test_parallel.rs::machine_parallel_status_returns_versioned_continuation_document",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_parallel.rs::machine_parallel_status_surfaces_stale_queue_lock_operator_state",
            "crates/ralph/tests/machine_contract_test/machine_contract_test_parallel.rs::machine_parallel_status_surfaces_blocked_worker_operator_state",
        ],
        &[
            "apps/RalphMac/RalphCoreTests/WorkspaceParallelRunControlTests.swift::test_runState_runControlOperatorState_classifiesQueueRecoveryActions",
            "apps/RalphMac/RalphCoreTests/WorkspaceParallelRunControlTests.swift::test_classifyParallelStatusActions_mapsNativeAndUnsupportedCommands",
            "apps/RalphMac/RalphCoreTests/WorkspaceParallelRunControlTests.swift::test_classifyParallelStatusActions_keepsDryRunStopAsCopyOnly",
        ],
        &[
            "Safe machine continuations should become native actions; unsupported ones must stay explicit copy/unsupported affordances.",
        ],
    ),
];

fn has_blank(items: &[&str]) -> bool {
    items.iter().any(|item| item.trim().is_empty())
}

pub fn app_parity_scenario_coverage_issues() -> Vec<AppParityCoverageIssue> {
    let mut issues = Vec::new();

    for entry in APP_PARITY_SCENARIO_REGISTRY {
        if entry.scenario.trim().is_empty() {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "scenario id must not be blank",
            });
        }
        if entry.owner_feature.trim().is_empty() {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "owner feature must not be blank",
            });
        }
        if entry.app_surface.trim().is_empty() {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "app surface must not be blank",
            });
        }
        if entry.machine_contract.is_empty() {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "at least one machine contract anchor is required",
            });
        } else if has_blank(entry.machine_contract) {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "machine contract anchors must not be blank",
            });
        }
        if entry.app_contract.is_empty() {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "at least one app contract anchor is required",
            });
        } else if has_blank(entry.app_contract) {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "app contract anchors must not be blank",
            });
        }
        if entry.rust_tests.is_empty() {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "at least one Rust proof anchor is required",
            });
        } else if has_blank(entry.rust_tests) {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "Rust proof anchors must not be blank",
            });
        }
        if entry.app_tests.is_empty() {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "at least one RalphMac proof anchor is required",
            });
        } else if has_blank(entry.app_tests) {
            issues.push(AppParityCoverageIssue {
                scenario: entry.scenario,
                problem: "RalphMac proof anchors must not be blank",
            });
        }
    }

    issues
}

pub fn app_parity_scenario_report() -> String {
    let issues = app_parity_scenario_coverage_issues();
    if issues.is_empty() {
        let mut lines = vec!["Scenario-level app parity coverage:".to_string()];
        for entry in APP_PARITY_SCENARIO_REGISTRY {
            lines.push(format!(
                "- {} [{:?}] machine:{} app:{} rust:{} swift:{}",
                entry.scenario,
                entry.status,
                entry.machine_contract.len(),
                entry.app_contract.len(),
                entry.rust_tests.len(),
                entry.app_tests.len()
            ));
        }
        return lines.join("\n");
    }

    let mut lines = vec!["Scenario-level app parity coverage is incomplete:".to_string()];
    for issue in issues {
        lines.push(format!("- {}: {}", issue.scenario, issue.problem));
    }
    lines.join("\n")
}

pub fn unclassified_human_cli_commands() -> Vec<String> {
    let registered = APP_PARITY_COMMAND_GUARD
        .iter()
        .copied()
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
