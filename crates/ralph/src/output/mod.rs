//! Output styling and theming for Ralph CLI.
//!
//! Purpose:
//! - Output styling and theming for Ralph CLI.
//!
//! Responsibilities:
//! - Provide centralized color theme definitions for CLI (colored crate).
//! - Export semantic color mappings for consistent styling across the application.
//!
//! Not handled here:
//! - Direct terminal output (see outpututil.rs for CLI output helpers).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Colors are semantic (success, error, warning) rather than literal (red, green).

pub mod theme;
