//! Plugin trait regression test hub.
//!
//! Purpose:
//! - Plugin trait regression test hub.
//!
//! Responsibilities:
//! - Share plugin-trait test imports and helpers across focused behavior groups.
//! - Keep the root test file small while preserving existing helper builders.
//!
//! Non-scope:
//! - Non-plugin execution tests covered by sibling runner test modules.
//! - Production plugin registration or runtime orchestration logic.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Child modules use `super::*` to access the shared builders and imports below.
//! - Helper contexts model the default CLI execution shape used by built-in plugins.

#![allow(clippy::needless_borrows_for_generic_args)]

use std::path::Path;

use crate::commands::run::PhaseType;
use crate::contracts::{Model, Runner, RunnerApprovalMode, RunnerSandboxMode};
use crate::runner::execution::plugin_trait::{ResumeContext, RunContext, RunnerMetadata};
use crate::runner::{
    OutputStream, ResolvedRunnerCliOptions,
    execution::{BuiltInRunnerPlugin, PluginExecutor, RunnerPlugin},
};

mod built_in;
mod command_building;
mod executor;
mod response_parsing;

fn create_run_context<'a>(prompt: &'a str, session_id: Option<&'a str>) -> RunContext<'a> {
    RunContext {
        work_dir: Path::new("."),
        bin: "test-runner",
        model: Model::Gpt53,
        prompt,
        timeout: None,
        output_handler: None,
        output_stream: OutputStream::HandlerOnly,
        runner_cli: ResolvedRunnerCliOptions::default(),
        reasoning_effort: None,
        permission_mode: None,
        phase_type: None,
        session_id: session_id.map(|s| s.to_string()),
    }
}

fn create_resume_context<'a>(session_id: &'a str, message: &'a str) -> ResumeContext<'a> {
    ResumeContext {
        work_dir: Path::new("."),
        bin: "test-runner",
        model: Model::Gpt53,
        session_id,
        message,
        timeout: None,
        output_handler: None,
        output_stream: OutputStream::HandlerOnly,
        runner_cli: ResolvedRunnerCliOptions::default(),
        reasoning_effort: None,
        permission_mode: None,
        phase_type: None,
    }
}
