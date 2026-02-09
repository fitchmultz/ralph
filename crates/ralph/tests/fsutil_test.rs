//! Tests for fsutil filesystem helpers (temp cleanup, atomic writes, and path normalization).
//!
//! Responsibilities:
//! - Validate temp directory cleanup and naming.
//! - Validate atomic write behavior for file content.
//! - Validate tilde (`~`) expansion to home directory.
//!
//! Not covered here:
//! - Directory locking behavior (see `lock_test.rs`).
//! - Queue semantics or CLI behavior.
//!
//! Invariants/assumptions:
//! - Tests operate in temp directories and may be run concurrently.
//! - Tests that modify HOME use `#[serial]` to prevent env race conditions.

use ralph::fsutil;
use serial_test::serial;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// Global lock for environment variable tests to prevent race conditions
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_cleanup_stale_temp_dirs_removes_prefixed_entries_only() {
    let base = TempDir::new().expect("create temp dir");
    let stale_dir = base
        .path()
        .join(format!("{}old-dir", fsutil::RALPH_TEMP_PREFIX));
    let stale_file = base
        .path()
        .join(format!("{}old-file.txt", fsutil::RALPH_TEMP_PREFIX));
    let keep_dir = base.path().join("keep-dir");

    fs::create_dir_all(&stale_dir).expect("create stale dir");
    fs::write(&stale_file, "stale").expect("create stale file");
    fs::create_dir_all(&keep_dir).expect("create keep dir");

    let removed = fsutil::cleanup_stale_temp_dirs(base.path(), Duration::from_secs(0))
        .expect("cleanup temp dirs");
    assert_eq!(removed, 2);
    assert!(!stale_dir.exists());
    assert!(!stale_file.exists());
    assert!(keep_dir.exists());
}

#[test]
fn test_create_ralph_temp_dir_uses_temp_root() {
    let temp_dir = fsutil::create_ralph_temp_dir("unit").expect("create ralph temp dir");
    let path = temp_dir.path().to_path_buf();
    let root = fsutil::ralph_temp_root();
    assert!(path.starts_with(&root));
    let name = path.file_name().expect("temp dir name").to_string_lossy();
    assert!(name.starts_with(fsutil::RALPH_TEMP_PREFIX));
}

#[test]
fn test_cleanup_stale_temp_entries_honors_prefix_list() {
    let base = TempDir::new().expect("create temp dir");
    let legacy_dir = base.path().join("legacy-temp");
    let ralph_dir = base
        .path()
        .join(format!("{}new-temp", fsutil::RALPH_TEMP_PREFIX));
    fs::create_dir_all(&legacy_dir).expect("create legacy dir");
    fs::create_dir_all(&ralph_dir).expect("create ralph dir");

    let removed = fsutil::cleanup_stale_temp_entries(
        base.path(),
        &["legacy", fsutil::RALPH_TEMP_PREFIX],
        Duration::from_secs(0),
    )
    .expect("cleanup temp entries");

    assert_eq!(removed, 2);
    assert!(!legacy_dir.exists());
    assert!(!ralph_dir.exists());
}

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
    let contents = vec![b'x'; 1024 * 1024]; // 1 MB of 'x'

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
    // Should fail because parent_path is a file, not a directory
    assert!(result.is_err());
}

#[test]
fn test_write_atomic_concurrent_writes() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("concurrent.txt");

    // Spawn multiple threads writing to the same file
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let path = file_path.clone();
            thread::spawn(move || {
                let contents = format!("writer-{}", i);
                fsutil::write_atomic(&path, contents.as_bytes())
            })
        })
        .collect();

    // Wait for all threads
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // At least some should succeed
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert!(success_count > 0);

    // File should exist and have valid content
    assert!(file_path.exists());
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.starts_with("writer-"));
}

#[test]
fn test_write_atomic_idempotent() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("idempotent.txt");
    let contents = b"same content";

    // Write the same content multiple times
    fsutil::write_atomic(&file_path, contents).unwrap();
    thread::sleep(Duration::from_millis(50)); // Ensure different timestamp
    fsutil::write_atomic(&file_path, contents).unwrap();
    thread::sleep(Duration::from_millis(50));
    fsutil::write_atomic(&file_path, contents).unwrap();

    let read_contents = fs::read(&file_path).unwrap();
    assert_eq!(read_contents, contents);
}

// Tests for expand_tilde

#[test]
#[serial]
fn expand_tilde_expands_tilde_to_home_when_home_set() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    // Set HOME to a known value
    unsafe { env::set_var("HOME", "/custom/home") };

    let result = fsutil::expand_tilde(PathBuf::from("~").as_path());
    assert_eq!(result, PathBuf::from("/custom/home"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_expands_tilde_slash_to_home_when_home_set() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let result = fsutil::expand_tilde(PathBuf::from("~/documents/file.txt").as_path());
    assert_eq!(result, PathBuf::from("/custom/home/documents/file.txt"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_returns_path_unchanged_when_home_unset() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    // Remove HOME
    unsafe { env::remove_var("HOME") };

    let result = fsutil::expand_tilde(PathBuf::from("~/documents").as_path());
    // Should return unchanged since HOME is not set
    assert_eq!(result, PathBuf::from("~/documents"));

    // Restore
    if let Some(v) = original_home {
        unsafe { env::set_var("HOME", v) }
    }
}

#[test]
#[serial]
fn expand_tilde_returns_path_unchanged_when_home_empty() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    // Set HOME to empty string
    unsafe { env::set_var("HOME", "") };

    let result = fsutil::expand_tilde(PathBuf::from("~/documents").as_path());
    // Should return unchanged since HOME is empty
    assert_eq!(result, PathBuf::from("~/documents"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_returns_path_unchanged_when_home_whitespace() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    // Set HOME to whitespace only
    unsafe { env::set_var("HOME", "   ") };

    let result = fsutil::expand_tilde(PathBuf::from("~/documents").as_path());
    // Should return unchanged since HOME is whitespace-only
    assert_eq!(result, PathBuf::from("~/documents"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_leaves_absolute_paths_unchanged() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let result = fsutil::expand_tilde(PathBuf::from("/absolute/path/to/file").as_path());
    assert_eq!(result, PathBuf::from("/absolute/path/to/file"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_leaves_nested_tilde_unchanged() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    // Tilde in the middle of path should NOT be expanded
    let result = fsutil::expand_tilde(PathBuf::from("/some/path/~/file").as_path());
    assert_eq!(result, PathBuf::from("/some/path/~/file"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_leaves_relative_paths_without_tilde_unchanged() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let result = fsutil::expand_tilde(PathBuf::from("relative/path/to/file").as_path());
    assert_eq!(result, PathBuf::from("relative/path/to/file"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_handles_tilde_with_double_slash() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    // Double slashes should be normalized
    let result = fsutil::expand_tilde(PathBuf::from("~//documents").as_path());
    assert_eq!(result, PathBuf::from("/custom/home/documents"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}

#[test]
#[serial]
fn expand_tilde_handles_tilde_slash_only() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let result = fsutil::expand_tilde(PathBuf::from("~/").as_path());
    assert_eq!(result, PathBuf::from("/custom/home"));

    // Restore
    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}
