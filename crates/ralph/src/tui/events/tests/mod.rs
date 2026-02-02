//! Event-handling tests for TUI keyboard and mouse interactions.
//!
//! This module is split into cohesive submodules, each focusing on a specific
//! area of functionality. See individual module docs for details.
//!
//! Module structure:
//! - `helpers`: Shared test utilities (key_event, make_test_task, etc.)
//! - `input`: Basic input handling and text character processing
//! - `quit`: Quit and discard flows
//! - `modes`: Mode transitions (Normal, Help, Search, etc.)
//! - `navigation`: List navigation, scrolling, and mouse handling
//! - `filters`: Search and filter functionality
//! - `palette`: Command palette behavior
//! - `task_ops`: Task operations (move, archive flow)
//! - `config`: Configuration editing and shortcuts
//! - `task_builder`: Task builder workflow
//! - `jump_to_task`: Jump-to-task feature
//! - `confirm_revert`: Confirm revert dialog
//! - `status_priority`: Status and priority commands
//! - `auto_archive`: Auto-archive behavior
//! - `resize`: Resize event handling

mod auto_archive;
mod config;
mod confirm_revert;
mod filters;
pub mod helpers;
mod input;
mod jump_to_task;
mod modes;
mod navigation;
mod palette;
mod quit;
mod resize;
mod status_priority;
mod task_builder;
mod task_ops;
