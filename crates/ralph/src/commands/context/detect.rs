//! Project type detection and hint conversion helpers.
//!
//! Purpose:
//! - Project type detection and hint conversion helpers.
//!
//! Responsibilities:
//! - Map CLI hints to detected project types and back.
//! - Infer project type from repo-root file heuristics.
//! - Report whether interactive terminal features are available.
//!
//! Not handled here:
//! - AGENTS.md rendering or persistence.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use super::types::DetectedProjectType;
use crate::cli::context::ProjectTypeHint;
use std::io::IsTerminal;
use std::path::Path;

pub(super) fn hint_to_detected(hint: ProjectTypeHint) -> DetectedProjectType {
    match hint {
        ProjectTypeHint::Rust => DetectedProjectType::Rust,
        ProjectTypeHint::Python => DetectedProjectType::Python,
        ProjectTypeHint::TypeScript => DetectedProjectType::TypeScript,
        ProjectTypeHint::Go => DetectedProjectType::Go,
        ProjectTypeHint::Generic => DetectedProjectType::Generic,
    }
}

pub(super) fn detected_type_to_hint(detected: DetectedProjectType) -> ProjectTypeHint {
    match detected {
        DetectedProjectType::Rust => ProjectTypeHint::Rust,
        DetectedProjectType::Python => ProjectTypeHint::Python,
        DetectedProjectType::TypeScript => ProjectTypeHint::TypeScript,
        DetectedProjectType::Go => ProjectTypeHint::Go,
        DetectedProjectType::Generic => ProjectTypeHint::Generic,
    }
}

pub(super) fn is_tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

pub(super) fn detect_project_type(repo_root: &Path) -> DetectedProjectType {
    if repo_root.join("Cargo.toml").exists() {
        return DetectedProjectType::Rust;
    }
    if repo_root.join("pyproject.toml").exists()
        || repo_root.join("setup.py").exists()
        || repo_root.join("requirements.txt").exists()
    {
        return DetectedProjectType::Python;
    }
    if repo_root.join("package.json").exists() {
        return DetectedProjectType::TypeScript;
    }
    if repo_root.join("go.mod").exists() {
        return DetectedProjectType::Go;
    }
    DetectedProjectType::Generic
}
