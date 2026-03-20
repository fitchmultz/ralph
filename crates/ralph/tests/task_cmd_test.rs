//! Purpose: thin integration-test hub for task command request parsing and option wiring coverage.
//!
//! Responsibilities:
//! - Re-export shared task command imports and contract enums for companion modules.
//! - Delegate request parsing, build option wiring, update setting wiring, and field comparison coverage to focused modules.
//!
//! Scope:
//! - Test-suite wiring only; this root module contains no test functions.
//!
//! Usage:
//! - Companion modules use `use super::*;` to access shared imports for `task_cmd`, contract types, and `Cursor`.
//!
//! Invariants/assumptions callers must respect:
//! - Test names, assertions, and coverage remain unchanged from the pre-split monolith.
//! - Tests continue using `read_request_from_args_or_reader` instead of real stdin so CI never blocks on terminal detection.

pub(crate) use ralph::commands::task as task_cmd;
pub(crate) use ralph::contracts::{Model, ReasoningEffort, Runner, RunnerCliOptionsPatch};
pub(crate) use std::io::Cursor;

#[path = "task_cmd_test/build_options.rs"]
mod build_options;
#[path = "task_cmd_test/compare_fields.rs"]
mod compare_fields;
#[path = "task_cmd_test/request_parsing.rs"]
mod request_parsing;
#[path = "task_cmd_test/update_settings.rs"]
mod update_settings;
