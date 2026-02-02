//! Output styling and theming for Ralph CLI and TUI.
//!
//! Responsibilities:
//! - Provide centralized color theme definitions for both CLI (colored crate)
//!   and TUI (ratatui) surfaces.
//! - Export semantic color mappings for consistent styling across the application.
//!
//! Not handled here:
//! - Direct terminal output (see outpututil.rs for CLI output helpers).
//! - TUI widget rendering (see tui/render/ for TUI-specific rendering).
//!
//! Invariants/assumptions:
//! - Colors are semantic (success, error, warning) rather than literal (red, green).
//! - Both CLI and TUI color mappings should be kept in sync.

pub mod theme;
