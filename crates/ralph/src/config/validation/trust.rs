//! Project execution trust validation.
//!
//! Purpose:
//! - Project execution trust validation.
//!
//! Responsibilities:
//! - Reject execution-sensitive project config when the repository is untrusted.
//! - Detect trust-relevant settings across the base agent, plugins, and profiles.
//!
//! Not handled here:
//! - Queue thresholds or git-ref validation.
//! - Repo trust-file loading.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Missing trust means the repo is untrusted.
//! - Execution-sensitive settings are intentionally broader than runner binaries alone.

use super::agent::agent_has_execution_settings;
use crate::config::{ConfigLayer, RepoTrust};
use anyhow::{Result, bail};

pub const ERR_PROJECT_EXECUTION_TRUST: &str = "Project config defines execution-sensitive settings (runner binaries, plugin runners, agent.ci_gate, and/or plugins), but this repo is not trusted. Run `ralph config trust init`, use `ralph init --trust-project-commands` when bootstrapping, create `.ralph/trust.jsonc` with {\"allow_project_commands\": true, \"trusted_at\": \"<RFC3339>\"}, or move those settings to trusted global config.";

pub fn validate_project_execution_trust(
    project_cfg: Option<&ConfigLayer>,
    repo_trust: &RepoTrust,
) -> Result<()> {
    let project_needs_trust = project_cfg.is_some_and(layer_has_execution_settings);
    if project_needs_trust && !repo_trust.is_trusted() {
        bail!(ERR_PROJECT_EXECUTION_TRUST);
    }
    Ok(())
}

fn layer_has_execution_settings(layer: &ConfigLayer) -> bool {
    if agent_has_execution_settings(&layer.agent) {
        return true;
    }
    if !layer.plugins.plugins.is_empty() {
        return true;
    }
    layer
        .profiles
        .as_ref()
        .is_some_and(|profiles| profiles.values().any(agent_has_execution_settings))
}
