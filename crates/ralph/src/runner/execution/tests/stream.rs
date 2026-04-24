//! Runner stream regression test hub.
//!
//! Purpose:
//! - Runner stream regression test hub.
//!
//! Responsibilities:
//! - Share stream-test imports across display and reader-focused coverage.
//! - Keep the root test file small while delegating behavior groups to companion files.
//!
//! Non-scope:
//! - Non-stream execution tests owned by sibling runner test modules.
//! - Production stream orchestration beyond the units exercised below.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Child modules use `super::*` for the shared stream helpers and imports below.
//! - Spawn helpers remain visible to this test module via the parent `stream` unit.

use super::super::stream::{StreamSink, display_filtered_json, extract_display_lines};
use crate::constants::buffers::{MAX_BUFFER_SIZE, MAX_LINE_LENGTH};
use crate::runner::{OutputHandler, OutputStream};
use serde_json::json;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use super::super::stream::{spawn_json_reader, spawn_reader};

mod display;
mod readers;
