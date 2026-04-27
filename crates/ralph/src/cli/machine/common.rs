//! Shared helpers for machine command handlers.
//!
//! Purpose:
//! - Shared helpers for machine command handlers.
//!
//! Responsibilities:
//! - Build shared machine documents reused across handlers.
//! - Centralize queue-path and config-safety shaping for machine responses.
//! - Reuse small queue/done helper semantics across machine subcommands.
//! - Convert operator-facing resume decisions into machine contract payloads.
//!
//! Not handled here:
//! - Clap argument definitions.
//! - JSON stdout/stderr emission.
//! - Queue/task/run command routing.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Machine config documents remain versioned through `crate::contracts` constants.
//! - Done-queue omission semantics match the existing machine/read-only behavior.
//! - Resume previews must be read-only and never mutate persisted session state.

use std::path::Path;

use anyhow::{Context, Result};

use crate::commands::runner::capabilities::built_in_runner_catalog;
use crate::config;
use crate::contracts::{
    GitPublishMode, GitRevertMode, MACHINE_CONFIG_RESOLVE_VERSION,
    MACHINE_WORKSPACE_OVERVIEW_VERSION, MachineConfigResolveDocument, MachineConfigSafetySummary,
    MachineExecutionControls, MachineParallelWorkersControl, MachineQueuePaths,
    MachineQueueReadDocument, MachineResumeDecision, MachineRunnerOption,
    MachineWorkspaceOverviewDocument, QueueFile,
};
use crate::plugins::discovery::PluginScope;
use crate::plugins::registry::PluginRegistry;
use crate::queue;
use crate::queue::operations::{RunnableSelectionOptions, queue_runnability_report};
use crate::session::{ResumeBehavior, ResumeDecisionMode, ResumeReason, ResumeScope, ResumeStatus};

const MACHINE_PARALLEL_MIN_WORKERS: u8 = 2;

pub(super) fn build_config_resolve_document(
    resolved: &config::Resolved,
    repo_trusted: bool,
    dirty_repo: bool,
    resume_preview: Option<MachineResumeDecision>,
) -> Result<MachineConfigResolveDocument> {
    Ok(MachineConfigResolveDocument {
        version: MACHINE_CONFIG_RESOLVE_VERSION,
        paths: queue_paths(resolved),
        safety: MachineConfigSafetySummary {
            repo_trusted,
            dirty_repo,
            git_publish_mode: resolved
                .config
                .agent
                .effective_git_publish_mode()
                .unwrap_or(GitPublishMode::Off),
            approval_mode: resolved.config.agent.effective_approval_mode(),
            ci_gate_enabled: resolved.config.agent.ci_gate_enabled(),
            git_revert_mode: resolved
                .config
                .agent
                .git_revert_mode
                .unwrap_or(GitRevertMode::Ask),
            parallel_configured: resolved.config.parallel.workers.is_some(),
            execution_interactivity: "noninteractive_streaming".to_string(),
            interactive_approval_supported: false,
        },
        config: resolved.config.clone(),
        execution_controls: build_execution_controls(resolved)?,
        resume_preview,
    })
}

pub(super) fn machine_safety_context(resolved: &config::Resolved) -> Result<(bool, bool)> {
    let repo_trust = config::load_repo_trust(&resolved.repo_root)?;
    let dirty_repo = crate::git::status_porcelain(&resolved.repo_root)
        .map(|status| !status.trim().is_empty())
        .unwrap_or(false);
    Ok((repo_trust.is_trusted(), dirty_repo))
}

pub(super) fn build_queue_read_document(
    resolved: &config::Resolved,
) -> Result<MachineQueueReadDocument> {
    let active = queue::load_queue(&resolved.queue_path)?;
    let done = queue::load_queue_or_default(&resolved.done_path)?;
    let done_ref = done_queue_ref(&done, &resolved.done_path);
    let options = RunnableSelectionOptions::new(false, true);
    let runnability = queue_runnability_report(&active, done_ref, options)?;
    let next_runnable_task_id =
        queue::operations::next_runnable_task(&active, done_ref).map(|task| task.id.clone());

    Ok(MachineQueueReadDocument {
        version: crate::contracts::MACHINE_QUEUE_READ_VERSION,
        paths: queue_paths(resolved),
        active,
        done,
        next_runnable_task_id,
        runnability: serde_json::to_value(runnability)?,
    })
}

pub(super) fn build_workspace_overview_document(
    resolved: &config::Resolved,
    repo_trusted: bool,
    dirty_repo: bool,
    resume_preview: Option<MachineResumeDecision>,
) -> Result<MachineWorkspaceOverviewDocument> {
    Ok(MachineWorkspaceOverviewDocument {
        version: MACHINE_WORKSPACE_OVERVIEW_VERSION,
        queue: build_queue_read_document(resolved)?,
        config: build_config_resolve_document(resolved, repo_trusted, dirty_repo, resume_preview)?,
    })
}

fn build_execution_controls(resolved: &config::Resolved) -> Result<MachineExecutionControls> {
    let mut runners: Vec<MachineRunnerOption> = built_in_runner_catalog()
        .into_iter()
        .map(|entry| MachineRunnerOption {
            id: entry.id,
            display_name: entry.display_name,
            source: "built_in".to_string(),
            reasoning_effort_supported: entry.reasoning_effort_supported,
            supports_arbitrary_model: entry.supports_arbitrary_model,
            allowed_models: entry.allowed_models,
            default_model: entry.default_model,
        })
        .collect();

    match PluginRegistry::load(&resolved.repo_root, &resolved.config) {
        Ok(registry) => {
            for (plugin_id, discovered) in registry.discovered() {
                if !registry.is_enabled(plugin_id) {
                    continue;
                }
                let Some(runner) = discovered.manifest.runner.as_ref() else {
                    continue;
                };
                if runners
                    .iter()
                    .any(|existing| existing.id.eq_ignore_ascii_case(plugin_id))
                {
                    log::warn!(
                        "Skipping plugin runner '{}' in machine execution controls because its id conflicts with an existing runner id",
                        plugin_id
                    );
                    continue;
                }
                runners.push(MachineRunnerOption {
                    id: plugin_id.clone(),
                    display_name: discovered.manifest.name.clone(),
                    source: plugin_source_label(discovered.scope).to_string(),
                    reasoning_effort_supported: false,
                    supports_arbitrary_model: true,
                    allowed_models: Vec::new(),
                    default_model: runner.default_model.clone(),
                });
            }
        }
        Err(err) => {
            log::warn!(
                "Failed to load plugin registry while building machine execution controls; falling back to built-in runners only: {err:#}"
            );
        }
    }

    Ok(MachineExecutionControls {
        runners,
        reasoning_efforts: vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
        ],
        parallel_workers: MachineParallelWorkersControl {
            min: MACHINE_PARALLEL_MIN_WORKERS,
            max: u8::MAX,
            default_missing_value: MACHINE_PARALLEL_MIN_WORKERS,
        },
    })
}

fn plugin_source_label(scope: PluginScope) -> &'static str {
    match scope {
        PluginScope::Global => "global_plugin",
        PluginScope::Project => "project_plugin",
    }
}

pub(super) fn build_resume_preview(
    resolved: &config::Resolved,
    explicit_task_id: Option<&str>,
    auto_resume: bool,
    non_interactive: bool,
    announce_missing_session: bool,
) -> anyhow::Result<Option<MachineResumeDecision>> {
    let queue_file = crate::queue::load_queue(&resolved.queue_path)?;
    let resolution = crate::session::resolve_run_session_decision(
        &resolved.repo_root.join(".ralph/cache"),
        &queue_file,
        crate::session::RunSessionDecisionOptions {
            timeout_hours: resolved.config.agent.session_timeout_hours,
            behavior: if auto_resume {
                ResumeBehavior::AutoResume
            } else {
                ResumeBehavior::Prompt
            },
            non_interactive,
            explicit_task_id,
            announce_missing_session,
            mode: ResumeDecisionMode::Preview,
        },
    )?;

    Ok(resolution
        .decision
        .as_ref()
        .map(machine_resume_decision_from_runtime))
}

pub(super) fn build_config_resolve_resume_preview(
    resolved: &config::Resolved,
) -> anyhow::Result<Option<MachineResumeDecision>> {
    match resolved
        .queue_path
        .try_exists()
        .with_context(|| format!("inspect queue file {}", resolved.queue_path.display()))?
    {
        false => Ok(None),
        true => build_resume_preview(resolved, None, true, true, false),
    }
}

pub(super) fn machine_resume_decision_from_runtime(
    decision: &crate::session::ResumeDecision,
) -> MachineResumeDecision {
    MachineResumeDecision {
        status: machine_resume_status(decision.status).to_string(),
        scope: machine_resume_scope(decision.scope).to_string(),
        reason: machine_resume_reason(decision.reason).to_string(),
        task_id: decision.task_id.clone(),
        message: decision.message.clone(),
        detail: decision.detail.clone(),
    }
}

fn machine_resume_status(status: ResumeStatus) -> &'static str {
    match status {
        ResumeStatus::ResumingSameSession => "resuming_same_session",
        ResumeStatus::FallingBackToFreshInvocation => "falling_back_to_fresh_invocation",
        ResumeStatus::RefusingToResume => "refusing_to_resume",
    }
}

fn machine_resume_scope(scope: ResumeScope) -> &'static str {
    match scope {
        ResumeScope::RunSession => "run_session",
        ResumeScope::ContinueSession => "continue_session",
    }
}

fn machine_resume_reason(reason: ResumeReason) -> &'static str {
    match reason {
        ResumeReason::NoSession => "no_session",
        ResumeReason::SessionValid => "session_valid",
        ResumeReason::SessionTimedOutConfirmed => "session_timed_out_confirmed",
        ResumeReason::SessionStale => "session_stale",
        ResumeReason::SessionDeclined => "session_declined",
        ResumeReason::ResumeConfirmationRequired => "resume_confirmation_required",
        ResumeReason::SessionTimedOutRequiresConfirmation => {
            "session_timed_out_requires_confirmation"
        }
        ResumeReason::ExplicitTaskSelectionOverridesSession => {
            "explicit_task_selection_overrides_session"
        }
        ResumeReason::ResumeTargetMissing => "resume_target_missing",
        ResumeReason::ResumeTargetTerminal => "resume_target_terminal",
        ResumeReason::RunnerSessionInvalid => "runner_session_invalid",
        ResumeReason::MissingRunnerSessionId => "missing_runner_session_id",
    }
}

pub(super) fn done_queue_ref<'a>(done: &'a QueueFile, done_path: &Path) -> Option<&'a QueueFile> {
    if done.tasks.is_empty() && !done_path.exists() {
        None
    } else {
        Some(done)
    }
}

pub(super) fn queue_paths(resolved: &config::Resolved) -> MachineQueuePaths {
    MachineQueuePaths {
        repo_root: resolved.repo_root.display().to_string(),
        queue_path: resolved.queue_path.display().to_string(),
        done_path: resolved.done_path.display().to_string(),
        project_config_path: resolved
            .project_config_path
            .as_ref()
            .map(|path| path.display().to_string()),
        global_config_path: resolved
            .global_config_path
            .as_ref()
            .map(|path| path.display().to_string()),
    }
}

pub(super) fn queue_max_dependency_depth(resolved: &config::Resolved) -> u8 {
    resolved.config.queue.max_dependency_depth.unwrap_or(10)
}

pub(crate) fn machine_queue_validate_command() -> &'static str {
    "ralph machine queue validate"
}

pub(crate) fn machine_queue_graph_command() -> &'static str {
    "ralph machine queue graph"
}

pub(crate) fn machine_queue_repair_command(dry_run: bool) -> &'static str {
    if dry_run {
        "ralph machine queue repair --dry-run"
    } else {
        "ralph machine queue repair"
    }
}

pub(crate) fn machine_queue_undo_dry_run_command() -> &'static str {
    "ralph machine queue undo --dry-run"
}

pub(crate) fn machine_queue_undo_restore_command() -> &'static str {
    "ralph machine queue undo --id <SNAPSHOT_ID>"
}

pub(crate) fn machine_task_mutate_command(dry_run: bool) -> &'static str {
    if dry_run {
        "ralph machine task mutate --dry-run --input <PATH>"
    } else {
        "ralph machine task mutate --input <PATH>"
    }
}

pub(crate) fn machine_task_build_command() -> &'static str {
    "ralph machine task build --input <PATH>"
}

pub(crate) fn machine_task_decompose_command(write: bool, suffix: &'static str) -> String {
    if write {
        format!("ralph machine task decompose --write {suffix}")
    } else {
        format!("ralph machine task decompose {suffix}")
    }
}

pub(crate) fn machine_run_one_resume_command() -> &'static str {
    "ralph machine run one --resume"
}

pub(crate) fn machine_run_stop_command(dry_run: bool) -> &'static str {
    if dry_run {
        "ralph machine run stop --dry-run"
    } else {
        "ralph machine run stop"
    }
}

pub(crate) fn machine_run_parallel_status_command() -> &'static str {
    "ralph machine run parallel-status"
}

pub(crate) fn machine_run_loop_command(parallel: bool, force: bool) -> &'static str {
    match (parallel, force) {
        (true, false) => "ralph machine run loop --resume --max-tasks 0 --parallel <N>",
        (true, true) => "ralph machine run loop --resume --max-tasks 0 --force --parallel <N>",
        (false, false) => "ralph machine run loop --resume --max-tasks 0",
        (false, true) => "ralph machine run loop --resume --max-tasks 0 --force",
    }
}

pub(crate) fn machine_doctor_report_command() -> &'static str {
    "ralph machine doctor report"
}

#[cfg(test)]
mod tests {
    use super::{MACHINE_PARALLEL_MIN_WORKERS, build_execution_controls};
    use crate::config::Resolved;
    use crate::contracts::{Config, PluginConfig};
    use tempfile::TempDir;

    fn resolved_for_repo(repo_root: &std::path::Path, config: Config) -> Resolved {
        Resolved {
            config,
            repo_root: repo_root.to_path_buf(),
            queue_path: repo_root.join(".ralph/queue.jsonc"),
            done_path: repo_root.join(".ralph/done.jsonc"),
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path: Some(repo_root.join(".ralph/config.jsonc")),
        }
    }

    fn write_runner_plugin(repo_root: &std::path::Path, plugin_id: &str, name: &str) {
        let plugin_dir = repo_root.join(".ralph/plugins").join(plugin_id);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.json"),
            format!(
                r#"{{
  "api_version": 1,
  "id": "{plugin_id}",
  "version": "1.0.0",
  "name": "{name}",
  "runner": {{
    "bin": "runner.sh",
    "default_model": "plugin-default"
  }}
}}"#
            ),
        )
        .unwrap();
    }

    fn trust_repo(repo_root: &std::path::Path) {
        let ralph_dir = repo_root.join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();
        std::fs::write(
            ralph_dir.join("trust.jsonc"),
            r#"{"allow_project_commands": true}"#,
        )
        .unwrap();
    }

    fn enabled_plugin_config(plugin_id: &str) -> Config {
        let mut config = Config::default();
        config.plugins.plugins.insert(
            plugin_id.to_string(),
            PluginConfig {
                enabled: Some(true),
                ..Default::default()
            },
        );
        config
    }

    #[test]
    fn execution_controls_include_enabled_trusted_project_plugin_runner() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        write_runner_plugin(tmp.path(), "acme.runner", "Acme Runner");
        let resolved = resolved_for_repo(tmp.path(), enabled_plugin_config("acme.runner"));

        let controls = build_execution_controls(&resolved).unwrap();
        let plugin = controls
            .runners
            .iter()
            .find(|runner| runner.id == "acme.runner")
            .expect("trusted enabled project plugin runner should be visible");

        assert_eq!(plugin.display_name, "Acme Runner");
        assert_eq!(plugin.source, "project_plugin");
        assert_eq!(plugin.default_model.as_deref(), Some("plugin-default"));
        assert!(plugin.supports_arbitrary_model);
        assert!(!plugin.reasoning_effort_supported);
    }

    #[test]
    fn execution_controls_hide_untrusted_project_plugin_runner() {
        let tmp = TempDir::new().unwrap();
        write_runner_plugin(tmp.path(), "acme.runner", "Acme Runner");
        let resolved = resolved_for_repo(tmp.path(), enabled_plugin_config("acme.runner"));

        let controls = build_execution_controls(&resolved).unwrap();

        assert!(
            controls
                .runners
                .iter()
                .all(|runner| runner.id != "acme.runner")
        );
    }

    #[test]
    fn execution_controls_parallel_worker_contract_matches_cli_bounds() {
        let tmp = TempDir::new().unwrap();
        let resolved = resolved_for_repo(tmp.path(), Config::default());

        let controls = build_execution_controls(&resolved).unwrap();
        assert_eq!(controls.parallel_workers.min, MACHINE_PARALLEL_MIN_WORKERS);
        assert_eq!(
            controls.parallel_workers.default_missing_value,
            MACHINE_PARALLEL_MIN_WORKERS
        );
        assert_eq!(controls.parallel_workers.max, u8::MAX);
    }

    #[test]
    fn execution_controls_fall_back_to_built_ins_when_plugin_discovery_fails() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        let plugin_dir = tmp.path().join(".ralph/plugins/broken.runner");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.json"), "{not valid json").unwrap();
        let resolved = resolved_for_repo(tmp.path(), Config::default());

        let controls = build_execution_controls(&resolved).unwrap();

        assert!(controls.runners.iter().any(|runner| runner.id == "codex"));
        assert!(
            controls
                .runners
                .iter()
                .all(|runner| runner.id != "broken.runner")
        );
    }

    #[test]
    fn execution_controls_skip_plugin_runner_ids_that_conflict_with_built_ins() {
        let tmp = TempDir::new().unwrap();
        trust_repo(tmp.path());
        write_runner_plugin(tmp.path(), "CODEX", "Codex Shadow Plugin");
        let resolved = resolved_for_repo(tmp.path(), enabled_plugin_config("CODEX"));

        let controls = build_execution_controls(&resolved).unwrap();
        let codex_runners = controls
            .runners
            .iter()
            .filter(|runner| runner.id.eq_ignore_ascii_case("codex"))
            .count();

        assert_eq!(codex_runners, 1);
        assert!(
            controls
                .runners
                .iter()
                .all(|runner| runner.display_name != "Codex Shadow Plugin")
        );
    }
}
