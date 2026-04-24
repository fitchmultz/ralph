//! Stream reader regression coverage.
//!
//! Purpose:
//! - Stream reader regression coverage.
//!
//! Responsibilities:
//! - Verify buffer-limit enforcement and empty/partial input handling for stream readers.
//! - Lock down the configured maximum line and buffer sizes.
//!
//! Non-scope:
//! - Runner-specific display-line extraction.
//! - Higher-level execution supervision outside the spawned reader threads.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Reader threads should complete successfully for normal, empty, and bounded oversized inputs.
//! - Shared output buffers must never exceed configured hard limits.

use super::*;
use std::io::{self, Read};

struct ChunkedReader {
    chunks: Vec<Vec<u8>>,
    next: usize,
}

impl ChunkedReader {
    fn split_inside(input: &str, marker: &str, bytes_into_marker: usize) -> Self {
        let marker_start = input.find(marker).expect("marker in input");
        let split_at = marker_start + bytes_into_marker;
        Self {
            chunks: vec![
                input.as_bytes()[..split_at].to_vec(),
                input.as_bytes()[split_at..].to_vec(),
            ],
            next: 0,
        }
    }
}

impl Read for ChunkedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(chunk) = self.chunks.get(self.next) else {
            return Ok(0);
        };
        assert!(chunk.len() <= buf.len());
        buf[..chunk.len()].copy_from_slice(chunk);
        self.next += 1;
        Ok(chunk.len())
    }
}

#[test]
fn max_line_length_constant_is_10mb() {
    // Verify the constant is set to expected 10MB value (reduced from 100MB)
    assert_eq!(MAX_LINE_LENGTH, 10 * 1024 * 1024);
}

#[test]
fn max_buffer_size_constant_is_10mb() {
    // Verify the constant is set to expected 10MB value (reduced from 100MB)
    assert_eq!(MAX_BUFFER_SIZE, 10 * 1024 * 1024);
}

#[test]
fn spawn_json_reader_handles_normal_lines() {
    let input = r#"{"type":"text","part":{"text":"hello world"}}"#;
    let reader = Cursor::new(input.as_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.contains("hello world"));
}

#[test]
fn spawn_json_reader_enforces_line_length_limit() {
    // Create input that exceeds MAX_LINE_LENGTH without newlines
    // Use owned data to satisfy 'static requirement
    let oversized_data: Vec<u8> = vec![b'x'; MAX_LINE_LENGTH + 1000];
    let reader = Cursor::new(oversized_data);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    // The shared output buffer should not grow beyond MAX_BUFFER_SIZE.
    // Note: line_buf has MAX_LINE_LENGTH protection, but the shared buffer
    // has MAX_BUFFER_SIZE protection (both are 10MB in current config).
    let guard = buffer.lock().unwrap();
    assert!(guard.len() <= MAX_BUFFER_SIZE);
}

#[test]
fn spawn_json_reader_handles_multiple_lines_within_limit() {
    // Create multiple normal-sized lines
    let lines: Vec<String> = (0..100)
        .map(|i| format!(r#"{{"type":"text","part":{{"text":"line {}"}}}}"#, i))
        .collect();
    let input = lines.join("\n");
    // Use owned data to satisfy 'static requirement
    let reader = Cursor::new(input.into_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    // Buffer should contain all the input since lines are processed and cleared
    assert!(guard.contains("line 0"));
    assert!(guard.contains("line 99"));
}

#[test]
fn spawn_reader_enforces_buffer_limit() {
    // Create input that exceeds MAX_BUFFER_SIZE
    // Use owned data to satisfy 'static requirement
    let oversized_data: Vec<u8> = vec![b'x'; MAX_BUFFER_SIZE + 10000];
    let reader = Cursor::new(oversized_data);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let handle = spawn_reader(
        reader,
        StreamSink::Stderr,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    // The buffer should be truncated to MAX_BUFFER_SIZE
    let guard = buffer.lock().unwrap();
    assert!(guard.len() <= MAX_BUFFER_SIZE);
}

#[test]
fn spawn_reader_handles_normal_output() {
    let input = "Hello, world!\nThis is a test.\n";
    let reader = Cursor::new(input.as_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let handle = spawn_reader(
        reader,
        StreamSink::Stderr,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert_eq!(guard.as_str(), input);
}

#[test]
fn spawn_reader_preserves_utf8_split_across_reads() {
    let input = "plain stderr before 😀 after\n";
    let reader = ChunkedReader::split_inside(input, "😀", 2);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let handled = Arc::new(Mutex::new(Vec::new()));
    let handler: OutputHandler = Arc::new(Box::new({
        let handled = Arc::clone(&handled);
        move |text: &str| handled.lock().unwrap().push(text.to_string())
    }));

    let handle = spawn_reader(
        reader,
        StreamSink::Stderr,
        Arc::clone(&buffer),
        Some(handler),
        OutputStream::HandlerOnly,
    );

    handle.join().unwrap().unwrap();

    let guard = buffer.lock().unwrap();
    assert_eq!(guard.as_str(), input);
    assert!(!guard.contains('\u{FFFD}'));

    let handled = handled.lock().unwrap();
    assert_eq!(handled.concat(), input);
}

#[test]
fn spawn_json_reader_handles_empty_input() {
    let reader = Cursor::new(b"");
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.is_empty());
}

#[test]
fn spawn_reader_handles_empty_input() {
    let reader = Cursor::new(b"");
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));

    let handle = spawn_reader(
        reader,
        StreamSink::Stderr,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.is_empty());
}

#[test]
fn spawn_json_reader_handles_line_exactly_at_limit() {
    // Create a line that is exactly at MAX_LINE_LENGTH
    // Use owned data to satisfy 'static requirement
    let exact_size_data: Vec<u8> = vec![b'x'; MAX_LINE_LENGTH];
    let reader = Cursor::new(exact_size_data);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    // The buffer should contain exactly MAX_LINE_LENGTH characters
    let guard = buffer.lock().unwrap();
    assert_eq!(guard.len(), MAX_LINE_LENGTH);
}

#[test]
fn spawn_json_reader_handles_partial_line_at_eof() {
    // Create a partial line (no trailing newline) that should still be processed
    let partial_line = r#"{"type":"text","part":{"text":"partial"}}"#;
    let reader = Cursor::new(partial_line.as_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let guard = buffer.lock().unwrap();
    assert!(guard.contains("partial"));
}

#[test]
fn spawn_json_reader_preserves_utf8_split_across_reads() {
    let input = r#"{"type":"text","part":{"text":"json before 😀 after"}}"#.to_string() + "\n";
    let reader = ChunkedReader::split_inside(&input, "😀", 1);
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handled = Arc::new(Mutex::new(Vec::new()));
    let handler: OutputHandler = Arc::new(Box::new({
        let handled = Arc::clone(&handled);
        move |line: &str| handled.lock().unwrap().push(line.to_string())
    }));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        Some(handler),
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    handle.join().unwrap().unwrap();

    let guard = buffer.lock().unwrap();
    assert_eq!(guard.as_str(), input);
    assert!(!guard.contains('\u{FFFD}'));

    let handled = handled.lock().unwrap();
    assert_eq!(handled.as_slice(), ["json before 😀 after\n"]);
}

#[test]
fn spawn_json_reader_plain_line_calls_output_handler_with_newline() {
    let input = "plain line without json";
    let reader = Cursor::new(input.as_bytes());
    let buffer: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let session_id_buf: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let handled = Arc::new(Mutex::new(Vec::new()));
    let handler: OutputHandler = Arc::new(Box::new({
        let handled = Arc::clone(&handled);
        move |line: &str| handled.lock().unwrap().push(line.to_string())
    }));

    let handle = spawn_json_reader(
        reader,
        StreamSink::Stdout,
        Arc::clone(&buffer),
        Some(handler),
        OutputStream::HandlerOnly,
        session_id_buf,
    );

    let result = handle.join().unwrap();
    assert!(result.is_ok());

    let handled = handled.lock().unwrap();
    assert_eq!(handled.as_slice(), ["plain line without json\n"]);
}
