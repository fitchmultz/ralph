//! Config mutation helpers for integration tests.
//!
//! Purpose:
//! - Config mutation helpers for integration tests.
//!
//! Responsibilities:
//! - Rewrite project config fixtures for runner, CI gate, and parallel-mode scenarios.
//! - Keep config mutation logic centralized so tests do not hand-edit JSON repeatedly.
//! - Apply trust-gated project-command setup when tests introduce local binaries.
//!
//! Non-scope:
//! - Repo initialization or fake command creation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Config helpers assume `ralph init` or equivalent created `.ralph/config.jsonc`.
//! - Mutations are full cutover fixture rewrites for the targeted keys.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

/// Update `.ralph/config.jsonc` to set `agent.runner`, `agent.model`, and `agent.phases`.
pub fn configure_agent_runner_model_phases(
    dir: &Path,
    runner: &str,
    model: &str,
    phases: u8,
) -> Result<()> {
    let config_path = dir.join(".ralph/config.jsonc");
    let config_str = std::fs::read_to_string(&config_path).context("read .ralph/config.jsonc")?;
    let mut config: Value =
        serde_json::from_str(&config_str).context("parse .ralph/config.jsonc")?;

    if config.get("agent").is_none() {
        config["agent"] = serde_json::json!({});
    }

    let agent = config["agent"]
        .as_object_mut()
        .context("config.agent is not an object")?;
    agent.insert("runner".to_string(), serde_json::json!(runner));
    agent.insert("model".to_string(), serde_json::json!(model));
    agent.insert("phases".to_string(), serde_json::json!(phases));

    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize .ralph/config.jsonc")?,
    )
    .context("write .ralph/config.jsonc")?;
    Ok(())
}

pub fn configure_runner(
    dir: &Path,
    runner: &str,
    model: &str,
    bin_path: Option<&Path>,
) -> Result<()> {
    let config_path = dir.join(".ralph/config.jsonc");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).context("read config")?)
            .context("parse config")?;
    if config.get("agent").is_none() {
        config["agent"] = serde_json::json!({});
    }
    let agent = config
        .get_mut("agent")
        .and_then(|value| value.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("config missing agent section"))?;
    agent.insert("runner".to_string(), serde_json::json!(runner));
    agent.insert("model".to_string(), serde_json::json!(model));
    agent.insert("phases".to_string(), serde_json::json!(1));
    if let Some(path) = bin_path {
        let key = match runner {
            "codex" => "codex_bin",
            "opencode" => "opencode_bin",
            "gemini" => "gemini_bin",
            "claude" => "claude_bin",
            _ => return Err(anyhow::anyhow!("unsupported runner: {}", runner)),
        };
        agent.insert(
            key.to_string(),
            serde_json::json!(path.to_string_lossy().to_string()),
        );
    }
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize config")?,
    )
    .context("write config")?;
    if bin_path.is_some() {
        super::test_support_command::trust_project_commands(dir)?;
    }
    Ok(())
}

pub fn configure_parallel_test_runner(
    dir: &Path,
    runner: &str,
    model: &str,
    bin_path: &Path,
    max_push_attempts: u8,
) -> Result<()> {
    configure_runner(dir, runner, model, Some(bin_path))?;
    configure_parallel_for_direct_push_with_attempts(dir, max_push_attempts)?;
    configure_ci_gate(dir, None, Some(false))?;
    Ok(())
}

pub fn configure_ci_gate(dir: &Path, command: Option<&str>, enabled: Option<bool>) -> Result<()> {
    let config_path = dir.join(".ralph/config.jsonc");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).context("read config")?)
            .context("parse config")?;
    if config.get("agent").is_none() {
        config["agent"] = serde_json::json!({});
    }
    let agent = config
        .get_mut("agent")
        .and_then(|value| value.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("config missing agent section"))?;
    let ci_gate = agent
        .entry("ci_gate".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let ci_gate = ci_gate
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("agent.ci_gate is not an object"))?;
    if let Some(command) = command {
        ci_gate.insert(
            "argv".to_string(),
            serde_json::json!(command.split_whitespace().collect::<Vec<_>>()),
        );
    }
    if let Some(enabled) = enabled {
        ci_gate.insert("enabled".to_string(), serde_json::json!(enabled));
    }
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize config")?,
    )
    .context("write config")?;
    Ok(())
}

/// Configure parallel mode settings for tests.
pub fn configure_parallel_disabled(dir: &Path) -> Result<()> {
    let config_path = dir.join(".ralph/config.jsonc");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).context("read config")?)
            .context("parse config")?;

    if config.get("parallel").is_none() {
        config["parallel"] = serde_json::json!({});
    }
    config["parallel"]["workers"] = serde_json::json!(2);

    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize config")?,
    )
    .context("write config")?;
    Ok(())
}

/// Configure parallel mode for direct-push with an explicit retry cap.
pub fn configure_parallel_for_direct_push_with_attempts(
    dir: &Path,
    max_push_attempts: u8,
) -> Result<()> {
    let config_path = dir.join(".ralph/config.jsonc");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).context("read config")?)
            .context("parse config")?;

    if config.get("parallel").is_none() {
        config["parallel"] = serde_json::json!({});
    }
    config["parallel"]["workers"] = serde_json::json!(2);
    config["parallel"]["max_push_attempts"] = serde_json::json!(max_push_attempts);

    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("serialize config")?,
    )
    .context("write config")?;
    Ok(())
}

/// Configure parallel mode for direct-push.
pub fn configure_parallel_for_direct_push(dir: &Path) -> Result<()> {
    configure_parallel_for_direct_push_with_attempts(dir, 5)
}
