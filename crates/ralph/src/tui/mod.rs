//! Interactive Terminal UI for browsing and managing the task queue.
//!
//! Responsibilities:
//! - Wire TUI modules and expose the public TUI entrypoints/types.
//! - Provide the shared `App` state and configuration used by render/event layers.
//!
//! Not handled here:
//! - CLI argument parsing or command routing (see `crate::cli`).
//! - Rendering and event implementations (see `tui::render` and `tui::events`).
//!
//! Invariants/assumptions:
//! - `App` is the single source of truth for TUI state.
//! - Public exports remain cohesive to the TUI surface area.

mod app;
mod app_details;
mod app_execution;
mod app_filters;
mod app_help;
mod app_id_index;
mod app_logs;
pub use app_logs::LogOperations;
mod app_loop;
mod app_multi_select;
mod app_navigation;
mod app_options;
mod app_palette;
mod app_session;
mod app_tasks;
mod config_edit;
mod dependency_graph_cache;
mod events;
mod help;
mod input;
mod keymap;
mod render;
mod task_edit;
pub mod terminal;
mod textarea_input;

#[cfg(test)]
mod tests;

pub use crate::progress::ExecutionPhase;
pub use app::{App, prepare_tui_session, run_tui};
pub use app_details::{DetailsContext, DetailsContextMode, DetailsState};
pub use app_execution::{
    ExecutionState, RunningKind, calculate_completion_percentage, count_completed_phases,
    get_phase_elapsed,
};
pub use app_filters::{
    FilterKey, FilterManagementOperations, FilterManager, FilterOperations, FilterSnapshot,
    FilterState,
};
pub use app_multi_select::{MultiSelectOperations, MultiSelectState};
pub use app_navigation::{AppNavigation, NavigationOperations, NavigationState};
#[cfg(test)]
pub use app_options::FilterCacheStats;
pub use app_options::TuiOptions;
pub use app_palette::{build_palette_entries, filter_and_score_entries, scan_label};
pub use app_session::{SessionManager, SessionState};
pub use app_tasks::{
    AppTasks, AutoArchiveAction, MoveResult, TaskMovementOperations, TaskOperations,
};
pub use config_edit::{ConfigEntry, ConfigFieldKind, ConfigKey};
pub use dependency_graph_cache::DependencyGraphCache;
pub use events::{
    AppMode, ConfirmDiscardAction, PaletteCommand, PaletteEntry, ScoredPaletteEntry, TuiAction,
    handle_key_event,
};
pub use input::TextInput;
pub use render::draw_ui;
pub use task_edit::{TaskEditEntry, TaskEditKind};
pub use textarea_input::MultiLineInput;
