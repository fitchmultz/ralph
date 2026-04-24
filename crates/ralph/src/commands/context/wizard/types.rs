//! Shared data models for the AGENTS.md wizard.
//!
//! Purpose:
//! - Shared data models for the AGENTS.md wizard.
//!
//! Responsibilities:
//! - Define init-wizard configuration hints and result payloads.
//! - Define the update-wizard return shape consumed by the workflow layer.
//!
//! Not handled here:
//! - Prompting.
//! - Wizard step orchestration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Default command hints remain the canonical generated-command fallbacks.
//! - Result types stay aligned with `workflow.rs` and `render.rs` consumers.

use crate::cli::context::ProjectTypeHint;
use std::path::PathBuf;

/// Configuration hints collected during init wizard.
#[derive(Debug, Clone)]
pub(crate) struct ConfigHints {
    /// Project description to replace placeholder.
    pub(crate) project_description: Option<String>,
    /// CI command (default: make ci).
    pub(crate) ci_command: String,
    /// Build command (default: make build).
    pub(crate) build_command: String,
    /// Test command (default: make test).
    pub(crate) test_command: String,
    /// Lint command (default: make lint).
    pub(crate) lint_command: String,
    /// Format command (default: make format).
    pub(crate) format_command: String,
}

impl Default for ConfigHints {
    fn default() -> Self {
        Self {
            project_description: None,
            ci_command: "make ci".to_string(),
            build_command: "make build".to_string(),
            test_command: "make test".to_string(),
            lint_command: "make lint".to_string(),
            format_command: "make format".to_string(),
        }
    }
}

/// Result of the init wizard.
#[derive(Debug, Clone)]
pub(crate) struct InitWizardResult {
    /// Selected project type.
    pub(crate) project_type: ProjectTypeHint,
    /// Optional output path override.
    pub(crate) output_path: Option<PathBuf>,
    /// Config hints for customizing the generated content.
    pub(crate) config_hints: ConfigHints,
    /// Whether to confirm before writing.
    pub(crate) confirm_write: bool,
}

/// Result of the update wizard: section name -> new content.
pub(crate) type UpdateWizardResult = Vec<(String, String)>;
