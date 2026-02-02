//! TUI render tests module.
//!
//! Responsibilities:
//! - Organize tests by component (footer, header, overlays, panels, utils, flowchart).
//! - Provide shared test utilities via common module.
//!
//! Not handled here:
//! - Individual test implementations (see submodules).

pub mod common;
mod flowchart;
mod footer;
mod header;
mod overlays;
mod panels;
mod utils;
