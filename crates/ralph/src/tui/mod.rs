//! Interactive Terminal UI for browsing and managing the task queue.

mod app;
mod config_edit;
mod events;
mod render;
mod task_edit;

#[cfg(test)]
mod tests;

pub use app::{prepare_tui_session, run_tui, App, FilterState, RunningKind, TuiOptions};
pub use config_edit::{ConfigEntry, ConfigFieldKind, ConfigKey};
pub use events::{handle_key_event, AppMode, PaletteCommand, PaletteEntry, TuiAction};
pub use render::draw_ui;
pub use task_edit::{TaskEditEntry, TaskEditKind};
