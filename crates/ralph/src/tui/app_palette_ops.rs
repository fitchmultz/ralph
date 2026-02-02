//! Palette command execution for the TUI.
//!
//! Responsibilities:
//! - Execute palette commands and dispatch to appropriate handlers
//! - Coordinate between palette UI and app operations
//! - Handle command-specific validation and setup
//!
//! Not handled here:
//! - Palette entry building/filtering (see app_palette module)
//! - UI rendering of palette (see render module)
//! - Key event handling (see events module)
//!
//! Invariants/assumptions:
//! - Commands are validated before execution
//! - Runner state is checked before spawning new tasks
//! - Loop mode coordination happens here for run commands

use anyhow::Result;

use crate::tui::events::{PaletteCommand, TuiAction};

/// Trait for palette command execution.
pub trait PaletteOperations {
    /// Execute a palette command (also used by direct keybinds for consistency).
    fn execute_palette_command(
        &mut self,
        cmd: PaletteCommand,
        now_rfc3339: &str,
    ) -> Result<TuiAction>;
}

// Implementation for App is in app.rs to avoid circular dependencies
