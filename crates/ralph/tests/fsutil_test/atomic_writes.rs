//! Purpose: atomic-write integration coverage for `ralph::fsutil`.
//!
//! Responsibilities:
//! - Verify atomic writes create files, parent directories, and overwrites correctly.
//! - Verify atomic writes preserve exact content across empty, large, binary, and unicode payloads.
//! - Verify failure, concurrency, and idempotency behavior remain unchanged.
//!
//! Scope:
//! - `fsutil::write_atomic` integration tests only; temp cleanup and tilde expansion live elsewhere.
//!
//! Usage:
//! - Compiled through the `fsutil_test` hub and relies on its shared imports.
//!
//! Invariants/Assumptions:
//! - Tests operate on temp directories only.
//! - Assertions remain identical to the pre-split suite.

use super::*;

#[test]
fn test_write_atomic_creates_file() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("test.txt");
    let contents = b"hello world";

    fsutil::write_atomic(&file_path, contents).unwrap();

    assert!(file_path.exists());
    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents);
}

#[test]
fn test_write_atomic_creates_parent_dirs() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("nested").join("dir").join("test.txt");
    let contents = b"nested content";

    fsutil::write_atomic(&file_path, contents).unwrap();

    assert!(file_path.exists());
    assert!(file_path.parent().unwrap().exists());
    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents);
}

#[test]
fn test_write_atomic_overwrites_existing() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("test.txt");
    let contents1 = b"original";
    let contents2 = b"updated";

    fsutil::write_atomic(&file_path, contents1).unwrap();
    fsutil::write_atomic(&file_path, contents2).unwrap();

    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents2);
}

#[test]
fn test_write_atomic_empty_content() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("empty.txt");
    let contents = b"";

    fsutil::write_atomic(&file_path, contents).unwrap();

    assert!(file_path.exists());
    let read_contents = fs::read(&file_path).unwrap();
    assert!(read_contents.is_empty());
}

#[test]
fn test_write_atomic_large_file() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("large.txt");
    let contents = vec![b'x'; 1024 * 1024];

    fsutil::write_atomic(&file_path, &contents).unwrap();

    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents);
}

#[test]
fn test_write_atomic_binary_content() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("binary.bin");
    let contents: Vec<u8> = (0..256).map(|i| i as u8).collect();

    fsutil::write_atomic(&file_path, &contents).unwrap();

    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents);
}

#[test]
fn test_write_atomic_unicode_content() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("unicode.txt");
    let contents = "Hello 世界 🎉".as_bytes();

    fsutil::write_atomic(&file_path, contents).unwrap();

    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents);

    let read_string = fs::read_to_string(&file_path).unwrap();
    assert_eq!(read_string, "Hello 世界 🎉");
}

#[test]
fn test_write_atomic_without_parent_fails() {
    let dir = TempDir::new().expect("create temp dir");
    let parent_path = dir.path().join("parent-file");
    fs::write(&parent_path, b"parent").expect("write parent file");
    let file_path = parent_path.join("test.txt");
    let contents = b"test";

    let result = fsutil::write_atomic(&file_path, contents);
    assert!(result.is_err());
}

#[test]
fn test_write_atomic_concurrent_writes() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("concurrent.txt");

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let path = file_path.clone();
            thread::spawn(move || {
                let contents = format!("writer-{}", i);
                fsutil::write_atomic(&path, contents.as_bytes())
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert!(success_count > 0);

    assert!(file_path.exists());
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.starts_with("writer-"));
}

#[test]
fn test_write_atomic_idempotent() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("idempotent.txt");
    let contents = b"same content";

    fsutil::write_atomic(&file_path, contents).unwrap();
    fsutil::write_atomic(&file_path, contents).unwrap();
    fsutil::write_atomic(&file_path, contents).unwrap();

    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents);
}
