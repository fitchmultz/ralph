//! Debug log tests for runner stream readers.

use super::super::stream::{spawn_reader, StreamSink};
use crate::debuglog::{enable, reset_for_tests, test_lock};
use crate::runner::OutputStream;
use serial_test::serial;
use std::fs;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[test]
#[serial]
fn spawn_reader_writes_raw_chunks_to_debug_log() {
    let _guard = test_lock().lock().expect("debug log lock");
    reset_for_tests();
    let dir = tempdir().expect("tempdir");
    enable(dir.path()).expect("enable debug log");

    let payload = b"raw stderr chunk\nsecond line\n";
    let buffer = Arc::new(Mutex::new(String::new()));

    let handle = spawn_reader(
        Cursor::new(payload),
        StreamSink::Stderr,
        Arc::clone(&buffer),
        None,
        OutputStream::HandlerOnly,
    );
    handle.join().expect("join").expect("reader ok");

    let debug_log = dir.path().join(".ralph/logs/debug.log");
    let contents = fs::read_to_string(&debug_log).expect("read log");
    assert!(
        contents.contains("[RUNNER STDERR]"),
        "log contents: {contents}"
    );
    assert!(
        contents.contains("raw stderr chunk"),
        "log contents: {contents}"
    );
    assert!(contents.contains("second line"), "log contents: {contents}");
    reset_for_tests();
}
