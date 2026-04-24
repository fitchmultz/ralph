//! Shared types for AGENTS.md context commands.
//!
//! Purpose:
//! - Shared types for AGENTS.md context commands.
//!
//! Responsibilities:
//! - Define public option and report structs used by the CLI surface.
//! - Define detected project type and file initialization state enums.
//!
//! Not handled here:
//! - Command execution logic.
//! - Template rendering or markdown parsing.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::cli::context::ProjectTypeHint;

/// Detected project type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedProjectType {
    Rust,
    Python,
    TypeScript,
    Go,
    Generic,
}

impl std::fmt::Display for DetectedProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectedProjectType::Rust => write!(f, "rust"),
            DetectedProjectType::Python => write!(f, "python"),
            DetectedProjectType::TypeScript => write!(f, "typescript"),
            DetectedProjectType::Go => write!(f, "go"),
            DetectedProjectType::Generic => write!(f, "generic"),
        }
    }
}

/// Status of file initialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileInitStatus {
    Created,
    Valid,
}

/// Options for context init command
pub struct ContextInitOptions {
    pub force: bool,
    pub project_type_hint: Option<ProjectTypeHint>,
    pub output_path: std::path::PathBuf,
    pub interactive: bool,
}

/// Options for context update command
pub struct ContextUpdateOptions {
    pub sections: Vec<String>,
    pub file: Option<std::path::PathBuf>,
    pub interactive: bool,
    pub dry_run: bool,
    pub output_path: std::path::PathBuf,
}

/// Options for context validate command
pub struct ContextValidateOptions {
    pub strict: bool,
    pub path: std::path::PathBuf,
}

/// Report from init command
pub struct InitReport {
    pub status: FileInitStatus,
    pub detected_project_type: DetectedProjectType,
    pub output_path: std::path::PathBuf,
}

/// Report from update command
pub struct UpdateReport {
    pub sections_updated: Vec<String>,
    pub dry_run: bool,
}

/// Report from validate command
pub struct ValidateReport {
    pub valid: bool,
    pub missing_sections: Vec<String>,
    pub outdated_sections: Vec<String>,
}
