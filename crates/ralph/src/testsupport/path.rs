//! PATH mutation guard + helpers for tests.
//!
//! Purpose:
//! - PATH mutation guard + helpers for tests.
//!
//! Responsibilities:
//! - Prevent concurrent `PATH` mutations across tests.
//! - Provide scoped PATH prepend helpers that always restore.
//! - Provide portable absolute-path fixtures for tests.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Not handled:
//! - Production PATH manipulation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - PATH is restored even if the closure returns error.

use std::sync::Mutex;

static PATH_GUARD: Mutex<()> = Mutex::new(());

/// Acquire the shared PATH guard used by tests that mutate or depend on PATH.
///
/// Tests that invoke PATH-resolved tools and must not overlap with fake-binary
/// PATH overrides should hold this guard for their full critical section.
pub(crate) fn path_lock() -> &'static Mutex<()> {
    &PATH_GUARD
}

/// Run a closure with a prepended path segment.
///
/// The PATH is restored after the closure completes, even if it panics or returns an error.
pub(crate) fn with_prepend_path<T>(prepend: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _guard = PATH_GUARD.lock().unwrap();
    let original = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", prepend.display(), original);
    // SAFETY: We hold the PATH_GUARD, so no other test can be mutating PATH concurrently.
    // We restore PATH in a drop guard to ensure it happens even on panic.
    struct PathGuard(String);
    impl Drop for PathGuard {
        fn drop(&mut self) {
            unsafe { std::env::set_var("PATH", &self.0) };
        }
    }
    let _path_guard = PathGuard(original);
    unsafe { std::env::set_var("PATH", &new_path) };
    f()
}

/// Run a closure with a specific PATH value.
///
/// The original PATH is restored after the closure completes.
pub(crate) fn with_path<T>(path_value: &str, f: impl FnOnce() -> T) -> T {
    let _guard = PATH_GUARD.lock().unwrap();
    let original = std::env::var("PATH").unwrap_or_default();
    struct PathGuard(String);
    impl Drop for PathGuard {
        fn drop(&mut self) {
            unsafe { std::env::set_var("PATH", &self.0) };
        }
    }
    let _path_guard = PathGuard(original);
    unsafe { std::env::set_var("PATH", path_value) };
    f()
}

/// Build a portable absolute path fixture rooted in the host temp directory.
///
/// The returned path is stable for assertions and does not assume a Unix `/tmp`
/// layout, so tests remain portable across platforms.
pub(crate) fn portable_abs_path(label: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    std::env::temp_dir().join("ralph-test-paths").join(label)
}
