//! Shared prompt preview option types.
//!
//! Purpose:
//! - Shared prompt preview option types.
//!
//! Responsibilities:
//! - Define preview modes and option structs shared across prompt helpers.
//! - Keep reusable public types separate from management and rendering code.
//!
//! Not handled here:
//! - Prompt construction logic.
//! - CLI parsing and config resolution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Worker prompt previews simulate runtime behavior closely.
//! - Explain flags only affect wrapper headers, not core prompt content.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    Phase1,
    Phase2,
    Phase3,
    Single,
}

#[derive(Debug, Clone)]
pub struct WorkerPromptOptions {
    pub task_id: Option<String>,
    pub mode: WorkerMode,
    pub repoprompt_plan_required: bool,
    pub repoprompt_tool_injection: bool,
    pub iterations: u8,
    pub iteration_index: u8,
    pub plan_file: Option<PathBuf>,
    pub plan_text: Option<String>,
    pub explain: bool,
}

#[derive(Debug, Clone)]
pub struct ScanPromptOptions {
    pub focus: String,
    pub mode: crate::cli::scan::ScanMode,
    pub repoprompt_tool_injection: bool,
    pub explain: bool,
}

#[derive(Debug, Clone)]
pub struct TaskBuilderPromptOptions {
    pub request: String,
    pub hint_tags: String,
    pub hint_scope: String,
    pub repoprompt_tool_injection: bool,
    pub explain: bool,
}
