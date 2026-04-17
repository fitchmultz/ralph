//! Streaming reader and display helpers for runner output.
//!
//! Responsibilities:
//! - Define the shared sink abstraction for stdout/stderr streaming.
//! - Re-export cohesive stream reader, rendering, and event helpers.
//!
//! Explicitly does NOT handle:
//! - Runner process lifecycle (see `super::command`).
//! - Output redaction (see `crate::redaction`).
//! - Debug logging policy outside stream readers.
//!
//! Invariants/Assumptions:
//! - Readers preserve valid UTF-8 across read boundaries.
//! - Invalid or incomplete EOF bytes are decoded lossily.
//! - JSON parsing remains best-effort; non-JSON lines are passed through.

use std::io::Write;

use crate::runner::OutputStream;

#[path = "stream_buffer.rs"]
mod stream_buffer;
#[path = "stream_events/mod.rs"]
mod stream_events;
#[path = "stream_reader.rs"]
mod stream_reader;
#[path = "stream_render.rs"]
mod stream_render;
#[path = "stream_tool_details.rs"]
mod stream_tool_details;

#[cfg(test)]
pub(crate) use stream_events::extract_display_lines;
pub(crate) use stream_reader::{spawn_json_reader, spawn_reader};
#[cfg(test)]
pub(crate) use stream_render::display_filtered_json;

pub(super) enum StreamSink {
    Stdout,
    Stderr,
}

impl StreamSink {
    pub(super) fn write_all(
        &self,
        bytes: &[u8],
        output_stream: OutputStream,
    ) -> std::io::Result<()> {
        if !output_stream.streams_to_terminal() {
            return Ok(());
        }
        match self {
            Self::Stdout => {
                let mut out = std::io::stdout().lock();
                out.write_all(bytes)?;
                out.flush()
            }
            Self::Stderr => {
                let mut err = std::io::stderr().lock();
                err.write_all(bytes)?;
                err.flush()
            }
        }
    }
}
