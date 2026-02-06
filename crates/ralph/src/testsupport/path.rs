//! PATH mutation guard + helpers for tests.
//!
//! Responsibilities:
//! - Prevent concurrent `PATH` mutations across tests.
//! - Provide scoped PATH prepend helpers that always restore.
//!
//! Not handled:
//! - Production PATH manipulation.
//!
//! Invariants:
//! - PATH is restored even if the closure returns error.

use std::sync::Mutex;

static PATH_GUARD: Mutex<()> = Mutex::new(());

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
