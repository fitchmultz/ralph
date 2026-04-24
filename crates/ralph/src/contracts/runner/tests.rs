//! Regression tests for runner contract parsing and serde behavior.
//!
//! Purpose:
//! - Regression tests for runner contract parsing and serde behavior.
//!
//! Responsibilities:
//! - Validate string token parsing for built-in and plugin runners.
//! - Cover normalized CLI enum parsing.
//! - Confirm serde/display invariants used by config and CLI surfaces.
//!
//! Not handled here:
//! - Runner execution behavior.
//! - Plugin registry or command dispatch.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - `Runner` serializes as a single string token.
//! - Hyphenated CLI enum inputs normalize to snake_case variants.

use super::{
    Runner, RunnerApprovalMode, RunnerOutputFormat, RunnerPlanMode, RunnerSandboxMode,
    RunnerVerbosity, UnsupportedOptionPolicy,
};

#[test]
fn runner_cli_enums_from_str_accept_hyphenated_tokens() {
    assert_eq!(
        "stream-json".parse::<RunnerOutputFormat>().unwrap(),
        RunnerOutputFormat::StreamJson
    );
    assert_eq!(
        "auto-edits".parse::<RunnerApprovalMode>().unwrap(),
        RunnerApprovalMode::AutoEdits
    );
    assert_eq!(
        "verbose".parse::<RunnerVerbosity>().unwrap(),
        RunnerVerbosity::Verbose
    );
    assert_eq!(
        "disabled".parse::<RunnerSandboxMode>().unwrap(),
        RunnerSandboxMode::Disabled
    );
    assert_eq!(
        "enabled".parse::<RunnerPlanMode>().unwrap(),
        RunnerPlanMode::Enabled
    );
    assert_eq!(
        "error".parse::<UnsupportedOptionPolicy>().unwrap(),
        UnsupportedOptionPolicy::Error
    );
}

#[test]
fn runner_parses_built_ins() {
    assert_eq!("codex".parse::<Runner>().unwrap(), Runner::Codex);
    assert_eq!("opencode".parse::<Runner>().unwrap(), Runner::Opencode);
    assert_eq!("gemini".parse::<Runner>().unwrap(), Runner::Gemini);
    assert_eq!("cursor".parse::<Runner>().unwrap(), Runner::Cursor);
    assert_eq!("claude".parse::<Runner>().unwrap(), Runner::Claude);
    assert_eq!("kimi".parse::<Runner>().unwrap(), Runner::Kimi);
    assert_eq!("pi".parse::<Runner>().unwrap(), Runner::Pi);
}

#[test]
fn runner_parses_plugin_id() {
    assert_eq!(
        "acme.super_runner".parse::<Runner>().unwrap(),
        Runner::Plugin("acme.super_runner".to_string())
    );
    assert_eq!(
        "my-custom-runner".parse::<Runner>().unwrap(),
        Runner::Plugin("my-custom-runner".to_string())
    );
}

#[test]
fn runner_rejects_empty() {
    assert!("".parse::<Runner>().is_err());
    assert!("   ".parse::<Runner>().is_err());
}

#[test]
fn runner_serde_roundtrip_is_string() {
    let runner = Runner::Plugin("acme.runner".to_string());
    let json = serde_json::to_string(&runner).unwrap();
    assert_eq!(json, "\"acme.runner\"");
    let back: Runner = serde_json::from_str(&json).unwrap();
    assert_eq!(runner, back);
}

#[test]
fn runner_built_in_serde_roundtrip() {
    let runner = Runner::Claude;
    let json = serde_json::to_string(&runner).unwrap();
    assert_eq!(json, "\"claude\"");
    let back: Runner = serde_json::from_str(&json).unwrap();
    assert_eq!(runner, back);
}

#[test]
fn runner_display_uses_id() {
    assert_eq!(Runner::Codex.to_string(), "codex");
    assert_eq!(
        Runner::Plugin("custom.runner".to_string()).to_string(),
        "custom.runner"
    );
}

#[test]
fn runner_is_plugin_detects_plugin_variant() {
    assert!(!Runner::Codex.is_plugin());
    assert!(!Runner::Claude.is_plugin());
    assert!(Runner::Plugin("x".to_string()).is_plugin());
}
