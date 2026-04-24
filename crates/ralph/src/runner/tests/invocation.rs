//! Runner invocation validation tests.
//!
//! Purpose:
//! - Runner invocation validation tests.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::commands::run::PhaseType;
use crate::contracts::{Model, ReasoningEffort, Runner};
use crate::runner::{OutputStream, RunnerBinaries, execution, resume_session, run_prompt};
use tempfile::tempdir;

#[test]
fn resume_session_missing_session_id_includes_runner_and_bin() {
    let dir = tempdir().expect("tempdir");
    let bins = RunnerBinaries {
        codex: "codex",
        opencode: "opencode",
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };

    let err = resume_session(
        Runner::Opencode,
        dir.path(),
        bins,
        Model::Glm47,
        None,
        execution::ResolvedRunnerCliOptions::default(),
        "   ",
        "hello",
        None,
        None,
        None,
        OutputStream::HandlerOnly,
        PhaseType::Implementation,
        None,
    )
    .unwrap_err();

    let msg = format!("{err}");
    assert!(msg.contains("operation=resume_session"));
    assert!(msg.contains("runner=opencode"));
    assert!(msg.contains("bin=opencode"));
    assert!(msg.to_lowercase().contains("session_id"));
}

#[test]
fn run_prompt_invalid_model_includes_operation_and_bin() {
    let dir = tempdir().expect("tempdir");
    let bins = RunnerBinaries {
        codex: "codex",
        opencode: "opencode",
        gemini: "gemini",
        claude: "claude",
        cursor: "agent",
        kimi: "kimi",
        pi: "pi",
    };

    let err = run_prompt(
        Runner::Codex,
        dir.path(),
        bins,
        Model::Glm47,
        Some(ReasoningEffort::Low),
        execution::ResolvedRunnerCliOptions::default(),
        "prompt",
        None,
        None,
        None,
        OutputStream::HandlerOnly,
        PhaseType::Implementation,
        None,
        None,
    )
    .unwrap_err();

    let msg = format!("{err}");
    assert!(msg.contains("operation=run_prompt"));
    assert!(msg.contains("runner=codex"));
    assert!(msg.contains("bin=codex"));
}

#[test]
fn semantic_failure_reason_detects_opencode_session_validation_error() {
    let stderr =
        r#"ZodError: [{"path":["sessionID"],"message":"Invalid string: must start with \"ses\""}]"#;
    let reason = super::super::invoke::semantic_failure_reason(&Runner::Opencode, stderr);
    assert_eq!(
        reason,
        Some("opencode rejected session_id during resume validation")
    );
}

#[test]
fn semantic_failure_reason_ignores_non_opencode_runners() {
    let stderr = "ZodError sessionID invalid_format must start with \"ses\"";
    let reason = super::super::invoke::semantic_failure_reason(&Runner::Gemini, stderr);
    assert_eq!(reason, None);
}
