//! Shared helpers for machine command handlers.
//!
//! Responsibilities:
//! - Build shared machine documents reused across handlers.
//! - Centralize queue-path and config-safety shaping for machine responses.
//! - Reuse small queue/done helper semantics across machine subcommands.
//!
//! Not handled here:
//! - Clap argument definitions.
//! - JSON stdout/stderr emission.
//! - Queue/task/run command routing.
//!
//! Invariants/assumptions:
//! - Machine config documents remain versioned through `crate::contracts` constants.
//! - Done-queue omission semantics match the existing machine/read-only behavior.

use std::path::Path;

use crate::config;
use crate::contracts::{
    GitPublishMode, GitRevertMode, MACHINE_CONFIG_RESOLVE_VERSION, MachineConfigResolveDocument,
    MachineConfigSafetySummary, MachineQueuePaths, QueueFile,
};

pub(super) fn build_config_resolve_document(
    resolved: &config::Resolved,
    repo_trusted: bool,
    dirty_repo: bool,
) -> MachineConfigResolveDocument {
    MachineConfigResolveDocument {
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
