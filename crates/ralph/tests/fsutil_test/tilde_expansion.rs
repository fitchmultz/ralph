//! Purpose: tilde-expansion integration coverage for `ralph::fsutil`.
//!
//! Responsibilities:
//! - Verify leading-tilde expansion uses HOME when available.
//! - Verify unsupported or invalid HOME states leave paths unchanged.
//! - Preserve serialized HOME-mutation behavior for env-dependent tests.
//!
//! Scope:
//! - `fsutil::expand_tilde` integration tests only; temp cleanup and atomic writes live elsewhere.
//!
//! Usage:
//! - Compiled through the `fsutil_test` hub and relies on shared imports plus `ENV_LOCK`.
//!
//! Invariants/Assumptions:
//! - Every HOME-mutating test is serialized with `#[serial]` and the shared `ENV_LOCK`.
//! - Assertions and restore logic remain identical to the pre-split suite.

use super::*;

#[test]
#[serial]
fn expand_tilde_expands_tilde_to_home_when_home_set() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "/custom/home") };

    let result = fsutil::expand_tilde(PathBuf::from("~").as_path());
    assert_eq!(result, PathBuf::from("/custom/home"));

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

    unsafe { env::remove_var("HOME") };

    let result = fsutil::expand_tilde(PathBuf::from("~/documents").as_path());
    assert_eq!(result, PathBuf::from("~/documents"));

    if let Some(v) = original_home {
        unsafe { env::set_var("HOME", v) }
    }
}

#[test]
#[serial]
fn expand_tilde_returns_path_unchanged_when_home_empty() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let original_home = env::var("HOME").ok();

    unsafe { env::set_var("HOME", "") };

    let result = fsutil::expand_tilde(PathBuf::from("~/documents").as_path());
    assert_eq!(result, PathBuf::from("~/documents"));

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

    unsafe { env::set_var("HOME", "   ") };

    let result = fsutil::expand_tilde(PathBuf::from("~/documents").as_path());
    assert_eq!(result, PathBuf::from("~/documents"));

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

    let result = fsutil::expand_tilde(PathBuf::from("/some/path/~/file").as_path());
    assert_eq!(result, PathBuf::from("/some/path/~/file"));

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

    let result = fsutil::expand_tilde(PathBuf::from("~//documents").as_path());
    assert_eq!(result, PathBuf::from("/custom/home/documents"));

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

    match original_home {
        Some(v) => unsafe { env::set_var("HOME", v) },
        None => unsafe { env::remove_var("HOME") },
    }
}
