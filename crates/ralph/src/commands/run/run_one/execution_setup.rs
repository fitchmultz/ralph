//! Task execution setup.
//!
//! Purpose:
//! - Task execution setup.
//!
//! Responsibilities:
//! - Setup for task execution after a task has been selected.
//! - Resolve phase count, iteration settings, phase matrix.
//! - Validate repo state, load plugins, mark task doing, create session.
//!
//! Not handled here:
//! - Context preparation (see context.rs).
//! - Phase execution (see phase_execution.rs).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Task has already been selected before calling setup.
//! - Repo must be clean (or force flag used) before proceeding.

use std::cell::RefCell;

use crate::agent::AgentOverrides;
use crate::config;
use crate::contracts::{AgentConfig, Task};
use crate::{git, runner, session};
use anyhow::{Context, Result, bail};

use super::orchestration::TaskExecutionSetup;
use crate::commands::run::{
    context::mark_task_doing, execution_timings::RunExecutionTimings,
    iteration::resolve_iteration_settings, phases::PostRunMode,
    run_session::create_session_for_task,
};
use crate::plugins::registry::PluginRegistry;

/// Setup task execution after a task has been selected.
///
/// Resolves phase count, iteration settings, phase matrix,
/// validates repo state, loads plugins, marks task doing, and creates session.
pub(crate) fn setup_task_execution<'a>(
    resolved: &'a config::Resolved,
    agent_overrides: &AgentOverrides,
    task: &Task,
    post_run_mode: PostRunMode,
    force: bool,
) -> Result<TaskExecutionSetup<'a>> {
    let phases = resolve_task_phase_count(agent_overrides, task, &resolved.config.agent)?;

    let iteration_settings = resolve_iteration_settings(task, &resolved.config.agent)?;
    log::info!(
        "RunOne: selected {} (phases={}, iterations={})",
        task.id.trim(),
        phases,
        iteration_settings.count
    );

    let (phase_matrix, phase_warnings) = runner::resolve_phase_settings_matrix(
        agent_overrides,
        &resolved.config.agent,
        task.agent.as_ref(),
        phases,
    )?;

    if phase_warnings.unused_phase1 {
        log::warn!(
            "Task {}: Phase 1 overrides specified but will not be used (phases < 2)",
            task.id.trim()
        );
    }
    if phase_warnings.unused_phase2 {
        log::warn!(
            "Task {}: Phase 2 overrides specified but will not be used (phases < 2 or single-phase mode)",
            task.id.trim()
        );
    }
    if phase_warnings.unused_phase3 {
        log::warn!(
            "Task {}: Phase 3 overrides specified but will not be used (phases < 3)",
            task.id.trim()
        );
    }

    log::info!("Task {}: Resolved phase settings:", task.id.trim());
    if phases >= 2 {
        log::info!(
            "  Phase 1 (Planning): runner={:?}, model={}",
            phase_matrix.phase1.runner,
            phase_matrix.phase1.model.as_str()
        );
    }
    log::info!(
        "  Phase 2 (Implementation): runner={:?}, model={}",
        phase_matrix.phase2.runner,
        phase_matrix.phase2.model.as_str()
    );
    if phases >= 3 {
        log::info!(
            "  Phase 3 (Review): runner={:?}, model={}",
            phase_matrix.phase3.runner,
            phase_matrix.phase3.model.as_str()
        );
    }

    let preexisting_dirty_allowed = git::repo_dirty_only_allowed_paths(
        &resolved.repo_root,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;
    git::require_clean_repo_ignoring_paths(
        &resolved.repo_root,
        force,
        git::RALPH_RUN_CLEAN_ALLOWED_PATHS,
    )?;

    let plugin_registry = PluginRegistry::load(&resolved.repo_root, &resolved.config)
        .context("load plugin registry")?;

    if !plugin_registry.discovered().is_empty() {
        let exec = crate::plugins::processor_executor::ProcessorExecutor::new(
            &resolved.repo_root,
            &plugin_registry,
        );
        exec.validate_task(task)
            .context("processor validate_task hook failed")?;
    }

    if matches!(post_run_mode, PostRunMode::ParallelWorker) {
        log::info!(
            "Task {}: parallel worker mode skips mark_task_doing to avoid queue writes",
            task.id.trim()
        );
    } else {
        mark_task_doing(resolved, &task.id)?;
    }

    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let session = create_session_for_task(
        &task.id,
        resolved,
        agent_overrides,
        iteration_settings.count,
        Some(&phase_matrix),
    );
    if let Err(e) = session::save_session(&cache_dir, &session) {
        log::warn!("Failed to save session state: {}", e);
    }

    let bins = runner::resolve_binaries(&resolved.config.agent);

    log::info!("Task {}: start", task.id.trim());

    let execution_timings: Option<RefCell<RunExecutionTimings>> =
        if post_run_mode == PostRunMode::ParallelWorker {
            None
        } else {
            Some(RefCell::new(RunExecutionTimings::default()))
        };

    Ok(TaskExecutionSetup {
        phases,
        iteration_settings,
        phase_matrix,
        preexisting_dirty_allowed,
        plugin_registry,
        bins,
        execution_timings,
    })
}

/// Resolve the phase count for a task.
fn resolve_task_phase_count(
    agent_overrides: &AgentOverrides,
    task: &Task,
    config_agent: &AgentConfig,
) -> Result<u8> {
    let phases = agent_overrides
        .phases
        .or(task.agent.as_ref().and_then(|agent| agent.phases))
        .or(config_agent.phases)
        .unwrap_or(2);

    if !(1..=3).contains(&phases) {
        bail!("Invalid phases value: {} (expected 1, 2, or 3)", phases);
    }

    Ok(phases)
}

#[cfg(test)]
mod tests {
    use super::resolve_task_phase_count;
    use crate::agent::AgentOverrides;
    use crate::contracts::{AgentConfig, Task, TaskAgent};

    #[test]
    fn resolve_task_phase_count_uses_cli_over_task_and_config() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "test".to_string(),
            ..Default::default()
        };
        task.agent = Some(TaskAgent {
            phases: Some(2),
            ..Default::default()
        });
        let config = AgentConfig {
            phases: Some(3),
            ..Default::default()
        };
        let overrides = AgentOverrides {
            phases: Some(1),
            ..Default::default()
        };

        let phases = resolve_task_phase_count(&overrides, &task, &config).expect("phases");
        assert_eq!(phases, 1);
    }

    #[test]
    fn resolve_task_phase_count_uses_task_when_cli_not_set() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "test".to_string(),
            ..Default::default()
        };
        task.agent = Some(TaskAgent {
            phases: Some(2),
            ..Default::default()
        });
        let config = AgentConfig {
            phases: Some(3),
            ..Default::default()
        };

        let phases =
            resolve_task_phase_count(&AgentOverrides::default(), &task, &config).expect("phases");
        assert_eq!(phases, 2);
    }

    #[test]
    fn resolve_task_phase_count_rejects_invalid_task_phase_value() {
        let mut task = Task {
            id: "RQ-0001".to_string(),
            title: "test".to_string(),
            ..Default::default()
        };
        task.agent = Some(TaskAgent {
            phases: Some(4),
            ..Default::default()
        });

        let err =
            resolve_task_phase_count(&AgentOverrides::default(), &task, &AgentConfig::default())
                .expect_err("expected invalid phases error");
        assert!(err.to_string().contains("Invalid phases value: 4"));
    }
}
