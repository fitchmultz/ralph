//! Runner capabilities reporting.
//!
//! Purpose:
//! - Runner capabilities reporting.
//!
//! Responsibilities:
//! - Aggregate capability data from multiple sources.
//! - Format output as text or JSON.
//!
//! Not handled here:
//! - Binary detection (see detection.rs).
//! - CLI argument parsing (see cli/runner.rs).
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use serde::Serialize;

use crate::cli::runner::RunnerFormat;
use crate::contracts::Runner;
use crate::runner::default_model_for_runner;
use crate::runner::{BuiltInRunnerPlugin, RunnerPlugin};

use super::detection::check_runner_binary;

/// Complete capability report for a runner.
#[derive(Debug, Clone, Serialize)]
pub struct RunnerCapabilityReport {
    /// Runner identifier.
    pub runner: String,
    /// Human-readable runner name.
    pub name: String,
    /// Whether session resumption is supported.
    pub supports_session_resume: bool,
    /// Whether Ralph must manage session IDs (e.g., Kimi).
    pub requires_managed_session_id: bool,
    /// Supported features.
    pub features: RunnerFeatures,
    /// Allowed models (None = arbitrary models allowed).
    pub allowed_models: Option<Vec<String>>,
    /// Default model for this runner.
    pub default_model: String,
    /// Binary status.
    pub binary: BinaryInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunnerFeatures {
    /// Reasoning effort control (Codex only).
    pub reasoning_effort: bool,
    /// Sandbox mode control.
    pub sandbox: SandboxSupport,
    /// Plan mode support (Cursor only).
    pub plan_mode: bool,
    /// Verbose output control.
    pub verbose: bool,
    /// Approval mode control.
    pub approval_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxSupport {
    pub supported: bool,
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BinaryInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

/// Get capabilities for a specific runner.
pub fn get_runner_capabilities(runner: &Runner, bin_name: &str) -> RunnerCapabilityReport {
    let plugin = runner_to_plugin(runner);
    let metadata = plugin.metadata();

    // Check binary status
    let binary_status = check_runner_binary(bin_name);
    let binary_info = BinaryInfo {
        installed: binary_status.installed,
        version: binary_status.version,
        error: binary_status.error,
    };

    // Get features based on runner type
    let features = get_runner_features(runner);

    // Get allowed models
    let allowed_models = get_allowed_models(runner);

    // Get default model
    let default_model = default_model_for_runner(runner);

    RunnerCapabilityReport {
        runner: runner.id().to_string(),
        name: metadata.name,
        supports_session_resume: metadata.supports_resume,
        requires_managed_session_id: plugin.requires_managed_session_id(),
        features,
        allowed_models,
        default_model: default_model.as_str().to_string(),
        binary: binary_info,
    }
}

fn runner_to_plugin(runner: &Runner) -> BuiltInRunnerPlugin {
    match runner {
        Runner::Codex => BuiltInRunnerPlugin::Codex,
        Runner::Opencode => BuiltInRunnerPlugin::Opencode,
        Runner::Gemini => BuiltInRunnerPlugin::Gemini,
        Runner::Claude => BuiltInRunnerPlugin::Claude,
        Runner::Kimi => BuiltInRunnerPlugin::Kimi,
        Runner::Pi => BuiltInRunnerPlugin::Pi,
        Runner::Cursor => BuiltInRunnerPlugin::Cursor,
        Runner::Plugin(_) => BuiltInRunnerPlugin::Claude, // Fallback
    }
}

pub(crate) fn get_runner_features(runner: &Runner) -> RunnerFeatures {
    match runner {
        Runner::Codex => RunnerFeatures {
            reasoning_effort: true,
            sandbox: SandboxSupport {
                supported: true,
                modes: vec!["default".into(), "enabled".into(), "disabled".into()],
            },
            plan_mode: false,
            verbose: false,
            approval_modes: vec!["config_file".into()], // Codex uses ~/.codex/config.json
        },
        Runner::Claude => RunnerFeatures {
            reasoning_effort: false,
            sandbox: SandboxSupport {
                supported: false,
                modes: vec![],
            },
            plan_mode: false,
            verbose: true,
            approval_modes: vec!["accept_edits".into(), "bypass_permissions".into()],
        },
        Runner::Gemini => RunnerFeatures {
            reasoning_effort: false,
            sandbox: SandboxSupport {
                supported: true,
                modes: vec!["default".into(), "enabled".into()],
            },
            plan_mode: false,
            verbose: false,
            approval_modes: vec!["yolo".into(), "auto_edit".into()],
        },
        Runner::Cursor => RunnerFeatures {
            reasoning_effort: false,
            sandbox: SandboxSupport {
                supported: true,
                modes: vec!["enabled".into(), "disabled".into()],
            },
            plan_mode: true,
            verbose: false,
            approval_modes: vec!["force".into()],
        },
        Runner::Opencode => RunnerFeatures {
            reasoning_effort: false,
            sandbox: SandboxSupport {
                supported: false,
                modes: vec![],
            },
            plan_mode: false,
            verbose: false,
            approval_modes: vec![],
        },
        Runner::Kimi => RunnerFeatures {
            reasoning_effort: false,
            sandbox: SandboxSupport {
                supported: false,
                modes: vec![],
            },
            plan_mode: false,
            verbose: false,
            approval_modes: vec!["yolo".into()],
        },
        Runner::Pi => RunnerFeatures {
            reasoning_effort: false,
            sandbox: SandboxSupport {
                supported: true,
                modes: vec!["default".into(), "enabled".into()],
            },
            plan_mode: false,
            verbose: false,
            approval_modes: vec!["print".into()],
        },
        Runner::Plugin(_) => RunnerFeatures {
            reasoning_effort: false,
            sandbox: SandboxSupport {
                supported: false,
                modes: vec![],
            },
            plan_mode: false,
            verbose: false,
            approval_modes: vec![],
        },
    }
}

fn get_allowed_models(runner: &Runner) -> Option<Vec<String>> {
    match runner {
        Runner::Codex => Some(vec![
            "gpt-5.4".into(),
            "gpt-5.3-codex".into(),
            "gpt-5.3-codex-spark".into(),
            "gpt-5.3".into(),
        ]),
        _ => None, // All other runners support arbitrary models
    }
}

/// Handle the `ralph runner capabilities` command.
pub fn handle_capabilities(runner_str: &str, format: RunnerFormat) -> anyhow::Result<()> {
    let runner: Runner = runner_str
        .parse()
        .map_err(|_| anyhow::anyhow!("unknown runner: {}", runner_str))?;

    let bin_name = get_bin_name(&runner);

    let report = get_runner_capabilities(&runner, &bin_name);

    match format {
        RunnerFormat::Text => print_capabilities_text(&report),
        RunnerFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
    }

    Ok(())
}

fn get_bin_name(runner: &Runner) -> String {
    match runner {
        Runner::Codex => "codex".into(),
        Runner::Opencode => "opencode".into(),
        Runner::Gemini => "gemini".into(),
        Runner::Claude => "claude".into(),
        Runner::Cursor => "agent".into(), // Cursor uses 'agent' binary
        Runner::Kimi => "kimi".into(),
        Runner::Pi => "pi".into(),
        Runner::Plugin(id) => id.clone(),
    }
}

fn print_capabilities_text(report: &RunnerCapabilityReport) {
    println!("Runner: {} ({})", report.name, report.runner);
    println!();

    // Binary status
    println!("Binary:");
    if report.binary.installed {
        println!("  Status: installed");
        if let Some(ref v) = report.binary.version {
            println!("  Version: {}", v);
        }
    } else {
        println!("  Status: NOT INSTALLED");
        if let Some(ref e) = report.binary.error {
            println!("  Error: {}", e);
        }
    }
    println!();

    // Models
    println!("Models:");
    println!("  Default: {}", report.default_model);
    if let Some(ref models) = report.allowed_models {
        println!("  Allowed: {}", models.join(", "));
    } else {
        println!("  Allowed: (any model ID)");
    }
    println!();

    // Features
    println!("Features:");
    println!(
        "  Session resume: {}",
        if report.supports_session_resume {
            "yes"
        } else {
            "no"
        }
    );
    if report.requires_managed_session_id {
        println!("  Managed session ID: required (Ralph supplies session ID)");
    }
    println!(
        "  Reasoning effort: {}",
        if report.features.reasoning_effort {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  Plan mode: {}",
        if report.features.plan_mode {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  Verbose output: {}",
        if report.features.verbose { "yes" } else { "no" }
    );

    // Sandbox
    if report.features.sandbox.supported {
        println!(
            "  Sandbox: {} (supported)",
            report.features.sandbox.modes.join(", ")
        );
    } else {
        println!("  Sandbox: not supported");
    }

    // Approval modes
    if !report.features.approval_modes.is_empty() {
        println!(
            "  Approval modes: {}",
            report.features.approval_modes.join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_has_reasoning_effort_support() {
        let features = get_runner_features(&Runner::Codex);
        assert!(features.reasoning_effort);
        assert!(!features.plan_mode);
    }

    #[test]
    fn cursor_has_plan_mode_support() {
        let features = get_runner_features(&Runner::Cursor);
        assert!(features.plan_mode);
        assert!(!features.reasoning_effort);
    }

    #[test]
    fn codex_has_restricted_models() {
        let report = get_runner_capabilities(&Runner::Codex, "codex");
        assert!(report.allowed_models.is_some());
        let models = report.allowed_models.unwrap();
        assert!(models.contains(&"gpt-5.4".to_string()));
        assert!(models.contains(&"gpt-5.3-codex".to_string()));
        assert!(!models.contains(&"sonnet".to_string()));
    }

    #[test]
    fn claude_allows_arbitrary_models() {
        let report = get_runner_capabilities(&Runner::Claude, "claude");
        assert!(report.allowed_models.is_none());
    }

    #[test]
    fn kimi_requires_managed_session_id() {
        let report = get_runner_capabilities(&Runner::Kimi, "kimi");
        assert!(report.requires_managed_session_id);
    }
}
