//! TUI modal/overlay rendering helpers.
//!
//! Responsibilities:
//! - Render modal overlays such as help, palettes, editors, and confirmations.
//! - Keep overlay layout consistent with TUI styling conventions.
//!
//! Not handled here:
//! - Event handling for overlay interaction.
//! - Base layout panels or footer rendering.
//!
//! Invariants/assumptions:
//! - Callers provide terminal areas sized for the current frame.
//! - Overlay drawing clears the underlying area before rendering content.

pub mod builder;
pub mod dialogs;
pub mod editors;
pub mod flowchart;
pub mod graph;
pub mod help;
pub mod jump;
pub mod palette;

// Re-exports for backward compatibility
pub use builder::draw_task_builder;
pub use dialogs::{draw_confirm_dialog, draw_revert_dialog, draw_risky_config_dialog};
pub use editors::{draw_config_editor, draw_task_editor};
pub use flowchart::draw_flowchart_overlay;
pub use graph::draw_dependency_graph_overlay;
pub use help::draw_help_overlay;
pub use jump::draw_jump_to_task_input;
pub use palette::draw_command_palette;
