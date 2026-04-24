//! Sink rendering helpers for runner JSON events.
//!
//! Purpose:
//! - Sink rendering helpers for runner JSON events.
//!
//! Responsibilities:
//! - Render normalized display lines to terminal sinks and optional handlers.
//! - Keep output fanout separate from event parsing and stream reading.
//!
//! Non-scope:
//! - JSON parsing or event correlation.
//! - Buffer management.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::runner::{OutputHandler, OutputStream};
use serde_json::Value as JsonValue;

use super::StreamSink;
use super::stream_events::extract_display_lines;

pub(crate) fn display_filtered_json(
    json: &JsonValue,
    sink: &StreamSink,
    output_handler: Option<&OutputHandler>,
    output_stream: OutputStream,
) -> anyhow::Result<()> {
    for mut line in extract_display_lines(json) {
        sink.write_all(line.as_bytes(), output_stream)?;
        sink.write_all(b"\n", output_stream)?;
        if let Some(handler) = output_handler {
            line.push('\n');
            handler(&line);
        }
    }

    Ok(())
}
