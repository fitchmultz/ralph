//! Runner configuration and binary checks for the doctor command.
//!
//! Responsibilities:
//! - Verify runner binary availability
//! - Check model compatibility with selected runner
//! - Validate instruction file configuration
//!
//! Not handled here:
//! - Runner execution (see runner module)
//! - Git repository checks (see git.rs)
//!
//! Invariants/assumptions:
//! - Runner binaries may have different flag conventions
//! - Plugin runners require separate validation

use crate::commands::doctor::types::{CheckResult, DoctorReport};
use crate::config;
use crate::contracts::Runner;
use crate::prompts;
use crate::runner;
use std::process::Command;

pub(crate) fn check_runner(report: &mut DoctorReport, resolved: &config::Resolved) {
    let runner = resolved.config.agent.runner.clone().unwrap_or_default();
    let runner_configured = runner_configured(resolved);
    let bin_name = match runner {
        Runner::Codex => resolved
            .config
            .agent
            .codex_bin
            .as_deref()
            .unwrap_or("codex"),
        Runner::Opencode => resolved
            .config
            .agent
            .opencode_bin
            .as_deref()
            .unwrap_or("opencode"),
        Runner::Gemini => resolved
            .config
            .agent
            .gemini_bin
            .as_deref()
            .unwrap_or("gemini"),
        Runner::Claude => resolved
            .config
            .agent
            .claude_bin
            .as_deref()
            .unwrap_or("claude"),
        Runner::Cursor => resolved
            .config
            .agent
            .cursor_bin
            .as_deref()
            .unwrap_or("agent"),
        Runner::Kimi => resolved.config.agent.kimi_bin.as_deref().unwrap_or("kimi"),
        Runner::Pi => resolved.config.agent.pi_bin.as_deref().unwrap_or("pi"),
        Runner::Plugin(_plugin_id) => {
            // For plugin runners, we can't determine the binary name from config
            // The plugin registry would need to be consulted
            return;
        }
    };

    if let Some((config_key, config_path)) = blocked_project_runner_override(resolved, &runner) {
        let message = format!(
            "project config defines execution-sensitive runner override '{}', but this repo is not trusted",
            config_key
        );
        let guidance = format!(
            "Move agent.{config_key} to trusted global config or create .ralph/trust.jsonc before running doctor checks that execute runner binaries. Config file: {}",
            config_path.display()
        );
        report.add(CheckResult::error(
            "runner",
            "runner_binary",
            &message,
            false,
            Some(&guidance),
        ));
        log::error!("{message}");
        log::error!("{guidance}");
        return;
    }

    if let Err(e) = check_runner_binary(bin_name) {
        let config_key = get_runner_config_key(&runner);
        let message = format!(
            "runner binary '{}' ({:?}) check failed: {}",
            bin_name, runner, e
        );

        if runner_configured {
            let result = CheckResult::error(
                "runner",
                "runner_binary",
                &message,
                false,
                Some(&format!(
                    "Install the runner binary, or configure a custom path in .ralph/config.jsonc: {{ \"agent\": {{ \"{}\": \"/path/to/{}\" }} }}",
                    config_key, bin_name
                )),
            );
            report.add(result);
            log::error!("");
            log::error!("To fix this issue:");
            log::error!("  1. Install the runner binary, or");
            log::error!("  2. Configure a custom path in .ralph/config.jsonc:");
            log::error!("     {{");
            log::error!("       \"agent\": {{");
            log::error!("         \"{}\": \"/path/to/{}\"", config_key, bin_name);
            log::error!("       }}");
            log::error!("     }}");
            log::error!("  3. Run 'ralph doctor' to verify the fix");
        } else {
            let result = CheckResult::warning(
                "runner",
                "runner_binary",
                &message,
                false,
                Some(
                    "Install the runner binary, or configure a custom path in .ralph/config.jsonc",
                ),
            );
            report.add(result);
            log::warn!("");
            log::warn!("To fix this issue:");
            log::warn!("  1. Install the runner binary, or");
            log::warn!("  2. Configure a custom path in .ralph/config.jsonc:");
            log::warn!("     {{");
            log::warn!("       \"agent\": {{");
            log::warn!("         \"{}\": \"/path/to/{}\"", config_key, bin_name);
            log::warn!("       }}");
            log::warn!("     }}");
            log::warn!("  3. Run 'ralph doctor' to verify the fix");
        }
    } else {
        report.add(CheckResult::success(
            "runner",
            "runner_binary",
            &format!("runner binary '{}' ({:?}) found", bin_name, runner),
        ));
    }

    // Model Compatibility Check
    let model = runner::resolve_model_for_runner(
        &runner,
        None,
        None,
        resolved.config.agent.model.clone(),
        false,
    );
    if let Err(e) = runner::validate_model_for_runner(&runner, &model) {
        report.add(CheckResult::error(
            "runner",
            "model_compatibility",
            &format!("config model/runner mismatch: {}", e),
            false,
            Some("Check the model is compatible with the selected runner in config"),
        ));
    } else {
        report.add(CheckResult::success(
            "runner",
            "model_compatibility",
            &format!(
                "model '{}' compatible with runner '{:?}'",
                model.as_str(),
                runner
            ),
        ));
    }

    // Instruction file injection checks
    let instruction_warnings =
        prompts::instruction_file_warnings(&resolved.repo_root, &resolved.config);

    // Check if repo AGENTS.md is explicitly configured
    let repo_agents_configured = resolved
        .config
        .agent
        .instruction_files
        .as_ref()
        .map(|files| {
            files.iter().any(|p| {
                let resolved = resolved.repo_root.join(p);
                resolved.ends_with("AGENTS.md")
            })
        })
        .unwrap_or(false);
    let repo_agents_path = resolved.repo_root.join("AGENTS.md");
    let repo_agents_exists = repo_agents_path.exists();

    if instruction_warnings.is_empty() {
        if let Some(files) = resolved.config.agent.instruction_files.as_ref()
            && !files.is_empty()
        {
            report.add(CheckResult::success(
                "runner",
                "instruction_files",
                &format!(
                    "instruction_files valid ({} configured file(s))",
                    files.len()
                ),
            ));
        }
        // Report status of repo AGENTS.md based on configuration
        if repo_agents_configured && repo_agents_exists {
            report.add(CheckResult::success(
                "runner",
                "agents_md",
                "AGENTS.md configured and readable",
            ));
        } else if repo_agents_exists && !repo_agents_configured {
            report.add(CheckResult::warning(
                "runner",
                "agents_md",
                "AGENTS.md exists at repo root but is not configured for injection. \
                 To enable, add 'AGENTS.md' to agent.instruction_files in your config.",
                false,
                Some("Add 'AGENTS.md' to agent.instruction_files in .ralph/config.jsonc"),
            ));
        }
    } else {
        for warning in instruction_warnings {
            report.add(CheckResult::warning(
                "runner",
                "instruction_files",
                &warning,
                false,
                Some("Check instruction file paths in config"),
            ));
        }
    }
}

fn blocked_project_runner_override(
    resolved: &config::Resolved,
    runner: &Runner,
) -> Option<(&'static str, std::path::PathBuf)> {
    let config_key = get_runner_config_key(runner);
    if config_key == "plugin_bin" {
        return None;
    }

    let repo_trust = config::load_repo_trust(&resolved.repo_root).ok()?;
    if repo_trust.is_trusted() {
        return None;
    }

    let project_path = resolved.project_config_path.as_ref()?;
    if !project_path.exists() {
        return None;
    }

    let layer = config::load_layer(project_path).ok()?;
    if runner_override_is_configured(&layer.agent, runner) {
        return Some((config_key, project_path.clone()));
    }

    None
}

fn runner_override_is_configured(agent: &crate::contracts::AgentConfig, runner: &Runner) -> bool {
    match runner {
        Runner::Codex => agent.codex_bin.is_some(),
        Runner::Opencode => agent.opencode_bin.is_some(),
        Runner::Gemini => agent.gemini_bin.is_some(),
        Runner::Claude => agent.claude_bin.is_some(),
        Runner::Cursor => agent.cursor_bin.is_some(),
        Runner::Kimi => agent.kimi_bin.is_some(),
        Runner::Pi => agent.pi_bin.is_some(),
        Runner::Plugin(_) => false,
    }
}

pub(crate) fn runner_configured(resolved: &config::Resolved) -> bool {
    let mut configured = false;
    let mut consider_layer = |path: &std::path::Path| {
        if configured {
            return;
        }
        let layer = match config::load_layer(path) {
            Ok(layer) => layer,
            Err(err) => {
                log::warn!("Unable to load config layer at {}: {}", path.display(), err);
                return;
            }
        };
        configured = layer.agent.runner.is_some()
            || layer.agent.codex_bin.is_some()
            || layer.agent.opencode_bin.is_some()
            || layer.agent.gemini_bin.is_some()
            || layer.agent.claude_bin.is_some();
    };

    if let Some(path) = resolved.global_config_path.as_ref()
        && path.exists()
    {
        consider_layer(path);
    }
    if let Some(path) = resolved.project_config_path.as_ref()
        && path.exists()
    {
        consider_layer(path);
    }

    configured
}

/// Check if a runner binary is executable by trying multiple common flags.
///
/// Tries the following in order:
/// 1. `--version`
/// 2. `-V`
/// 3. `--help`
/// 4. `help`
///
/// Returns Ok if any invocation succeeds.
pub(crate) fn check_runner_binary(bin: &str) -> anyhow::Result<()> {
    let fallbacks: &[&[&str]] = &[&["--version"], &["-V"], &["--help"], &["help"]];

    for args in fallbacks {
        match check_command(bin, args) {
            Ok(()) => return Ok(()),
            Err(_) => continue,
        }
    }

    Err(anyhow::anyhow!(
        "tried: {}",
        fallbacks
            .iter()
            .map(|a| a.join(" "))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Get the config key for a runner's binary path override.
pub(crate) fn get_runner_config_key(runner: &Runner) -> &'static str {
    match runner {
        Runner::Codex => "codex_bin",
        Runner::Opencode => "opencode_bin",
        Runner::Gemini => "gemini_bin",
        Runner::Claude => "claude_bin",
        Runner::Cursor => "cursor_bin",
        Runner::Kimi => "kimi_bin",
        Runner::Pi => "pi_bin",
        Runner::Plugin(_) => "plugin_bin",
    }
}

fn check_command(bin: &str, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_msg = if stderr.trim().is_empty() {
            format!(
                "command '{}' {:?} failed with exit status: {}",
                bin, args, output.status
            )
        } else {
            format!(
                "command '{}' {:?} failed with exit status {}: {}",
                bin,
                args,
                output.status,
                stderr.trim()
            )
        };
        Err(anyhow::anyhow!(stderr_msg))
    }
}
