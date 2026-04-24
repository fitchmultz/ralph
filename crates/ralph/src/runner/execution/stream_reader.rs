//! Raw and JSON subprocess reader loops for runner output streams.
//!
//! Purpose:
//! - Raw and JSON subprocess reader loops for runner output streams.
//!
//! Responsibilities:
//! - Read stdout/stderr incrementally from child processes.
//! - Apply shared buffer truncation rules.
//! - Parse JSON lines and forward rendered output.
//!
//! Non-scope:
//! - Event formatting internals (see `stream_events`).
//! - Sink rendering policy (see `stream_render`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Child output is read in fixed-size byte chunks; `Utf8ChunkDecoder` keeps trailing
//!   incomplete UTF-8 in a pending buffer so handlers and line assembly see whole
//!   scalar boundaries instead of spurious U+FFFD from mid-sequence splits.
//! - Truly invalid bytes and any incomplete trailing sequence at EOF are decoded
//!   lossily (replacement character only where the byte sequence is not valid UTF-8).

use anyhow::Context;
use std::io::Read;
use std::str;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::constants::buffers::MAX_LINE_LENGTH;
use crate::debuglog::{self, DebugStream};
use crate::runner::{OutputHandler, OutputStream};

use super::super::json::{extract_session_id_from_json, parse_json_line};
use super::StreamSink;
use super::stream_buffer::append_to_buffer;
use super::stream_events::ToolCallTracker;
use super::stream_render::display_filtered_json;

pub(crate) fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
) -> thread::JoinHandle<anyhow::Result<()>> {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut decoder = Utf8ChunkDecoder::default();
        let mut buffer_exceeded_logged = false;
        loop {
            let read = reader.read(&mut buf).context("read child output")?;
            if read == 0 {
                break;
            }
            sink.write_all(&buf[..read], output_stream)
                .context("stream child output")?;
            let text = decoder.decode(&buf[..read]);
            handle_raw_text(
                &text,
                &buffer,
                &mut buffer_exceeded_logged,
                output_handler.as_ref(),
            )?;
        }
        let text = decoder.finish();
        handle_raw_text(
            &text,
            &buffer,
            &mut buffer_exceeded_logged,
            output_handler.as_ref(),
        )?;
        Ok(())
    })
}

pub(crate) fn spawn_json_reader<R: Read + Send + 'static>(
    mut reader: R,
    sink: StreamSink,
    buffer: Arc<Mutex<String>>,
    output_handler: Option<OutputHandler>,
    output_stream: OutputStream,
    session_id_buf: Arc<Mutex<Option<String>>>,
) -> thread::JoinHandle<anyhow::Result<()>> {
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut decoder = Utf8ChunkDecoder::default();
        let mut state = JsonReaderState::default();

        loop {
            let read = reader.read(&mut buf).context("read child output")?;
            if read == 0 {
                break;
            }

            let text = decoder.decode(&buf[..read]);
            handle_json_text(
                &text,
                &sink,
                output_handler.as_ref(),
                output_stream,
                &session_id_buf,
                &buffer,
                &mut state,
            )?;
        }

        let text = decoder.finish();
        handle_json_text(
            &text,
            &sink,
            output_handler.as_ref(),
            output_stream,
            &session_id_buf,
            &buffer,
            &mut state,
        )?;

        if !state.line_buf.trim().is_empty() {
            if state.line_length_exceeded {
                log::warn!(
                    "Runner output line exceeded {}MB limit; truncating",
                    MAX_LINE_LENGTH / (1024 * 1024)
                );
            }
            handle_plain_line(
                &state.line_buf,
                &sink,
                output_handler.as_ref(),
                output_stream,
            )?;
        }
        Ok(())
    })
}

#[derive(Default)]
struct Utf8ChunkDecoder {
    pending: Vec<u8>,
}

#[derive(Default)]
struct JsonReaderState {
    line_buf: String,
    line_length_exceeded: bool,
    buffer_exceeded_logged: bool,
    tool_tracker: ToolCallTracker,
}

impl Utf8ChunkDecoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        let mut combined = Vec::with_capacity(self.pending.len() + bytes.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(bytes);
        self.pending.clear();
        self.decode_combined(&combined)
    }

    fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            String::new()
        } else {
            String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned()
        }
    }

    fn decode_combined(&mut self, bytes: &[u8]) -> String {
        let mut decoded = String::new();
        let mut remaining = bytes;
        loop {
            match str::from_utf8(remaining) {
                Ok(valid) => {
                    decoded.push_str(valid);
                    break;
                }
                Err(error) => {
                    let valid_up_to = error.valid_up_to();
                    if valid_up_to > 0 {
                        decoded.push_str(
                            str::from_utf8(&remaining[..valid_up_to])
                                .expect("valid prefix from UTF-8 error"),
                        );
                    }

                    if let Some(error_len) = error.error_len() {
                        decoded.push('\u{FFFD}');
                        remaining = &remaining[valid_up_to + error_len..];
                    } else {
                        self.pending.extend_from_slice(&remaining[valid_up_to..]);
                        break;
                    }
                }
            }
        }
        decoded
    }
}

fn handle_raw_text(
    text: &str,
    buffer: &Arc<Mutex<String>>,
    buffer_exceeded_logged: &mut bool,
    output_handler: Option<&OutputHandler>,
) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    debuglog::write_runner_chunk(DebugStream::Stderr, text);
    let mut guard = buffer
        .lock()
        .map_err(|_| anyhow::anyhow!("lock output buffer"))?;
    append_to_buffer(&mut guard, text, buffer_exceeded_logged);

    if let Some(handler) = output_handler {
        handler(text);
    }

    Ok(())
}

fn handle_json_text(
    text: &str,
    sink: &StreamSink,
    output_handler: Option<&OutputHandler>,
    output_stream: OutputStream,
    session_id_buf: &Arc<Mutex<Option<String>>>,
    buffer: &Arc<Mutex<String>>,
    state: &mut JsonReaderState,
) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    debuglog::write_runner_chunk(DebugStream::Stdout, text);
    for ch in text.chars() {
        if ch == '\n' {
            if state.line_length_exceeded {
                log::warn!(
                    "Runner output line exceeded {}MB limit; truncating",
                    MAX_LINE_LENGTH / (1024 * 1024)
                );
                state.line_length_exceeded = false;
            }
            handle_completed_line(
                &state.line_buf,
                sink,
                output_handler,
                output_stream,
                session_id_buf,
                &mut state.tool_tracker,
            )?;
            state.line_buf.clear();
        } else if state.line_buf.len() >= MAX_LINE_LENGTH {
            state.line_length_exceeded = true;
        } else {
            state.line_buf.push(ch);
        }
    }

    let mut guard = buffer
        .lock()
        .map_err(|_| anyhow::anyhow!("lock output buffer"))?;
    append_to_buffer(&mut guard, text, &mut state.buffer_exceeded_logged);

    Ok(())
}

fn handle_completed_line(
    line_buf: &str,
    sink: &StreamSink,
    output_handler: Option<&OutputHandler>,
    output_stream: OutputStream,
    session_id_buf: &Arc<Mutex<Option<String>>>,
    tool_tracker: &mut ToolCallTracker,
) -> anyhow::Result<()> {
    if let Some(mut json) = parse_json_line(line_buf) {
        tool_tracker.correlate(&mut json);
        if let Some(id) = extract_session_id_from_json(&json)
            && let Ok(mut guard) = session_id_buf.lock()
        {
            *guard = Some(id.to_owned());
        }
        display_filtered_json(&json, sink, output_handler, output_stream)?;
    } else if !line_buf.trim().is_empty() {
        handle_plain_line(line_buf, sink, output_handler, output_stream)?;
    }

    Ok(())
}

fn handle_plain_line(
    line: &str,
    sink: &StreamSink,
    output_handler: Option<&OutputHandler>,
    output_stream: OutputStream,
) -> anyhow::Result<()> {
    sink.write_all(line.as_bytes(), output_stream)?;
    sink.write_all(b"\n", output_stream)?;
    if let Some(handler) = output_handler {
        let mut emitted = String::with_capacity(line.len() + 1);
        emitted.push_str(line);
        emitted.push('\n');
        handler(&emitted);
    }
    Ok(())
}
