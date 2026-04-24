//! Deterministic synchronization helpers for integration tests.
//!
//! Purpose:
//! - Deterministic synchronization helpers for integration tests.
//!
//! Responsibilities:
//! - Provide bounded condition waiting without raw sleeps.
//! - Expose reusable cross-thread signaling for subprocess and async test coordination.
//! - Offer deterministic process-state helpers used by integration fixtures.
//!
//! Non-scope:
//! - Filesystem fixture setup or command execution.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Wait helpers are timeout-bounded and may return `false`/`None` instead of panicking.
//! - Poll intervals are treated as lower bounds and may back off up to 100ms.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Poll a condition until it succeeds or the timeout expires.
///
/// Uses adaptive polling: starts with the specified interval and exponentially
/// backs off up to 100ms. This keeps fast tests fast while reducing CPU
/// contention during longer waits.
pub fn wait_until(
    timeout: Duration,
    poll_interval: Duration,
    mut condition: impl FnMut() -> bool,
) -> bool {
    if condition() {
        return true;
    }

    let mut interval = poll_interval.max(Duration::from_millis(1));
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        std::thread::park_timeout(interval);
        if condition() {
            return true;
        }
        if interval < Duration::from_millis(100) {
            interval = interval.saturating_mul(2).min(Duration::from_millis(100));
        }
    }

    condition()
}

pub struct Signal<T> {
    value: Mutex<Option<T>>,
    ready: Condvar,
}

impl<T> Signal<T> {
    pub fn new() -> Self {
        Self {
            value: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    pub fn notify(&self, value: T) {
        let mut slot = self.value.lock().expect("lock signal");
        *slot = Some(value);
        self.ready.notify_all();
    }

    pub fn wait(&self, timeout: Duration) -> Option<T>
    where
        T: Clone,
    {
        let deadline = Instant::now() + timeout;
        let mut slot = self.value.lock().expect("lock signal");
        while slot.is_none() {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let (next_slot, wait_result) = self
                .ready
                .wait_timeout(slot, remaining)
                .expect("wait on signal condvar");
            slot = next_slot;
            if wait_result.timed_out() {
                break;
            }
        }
        slot.clone()
    }
}

impl<T> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Gate {
    open: Mutex<bool>,
    ready: Condvar,
}

impl Gate {
    pub fn new_closed() -> Self {
        Self {
            open: Mutex::new(false),
            ready: Condvar::new(),
        }
    }

    pub fn open(&self) {
        let mut open = self.open.lock().expect("lock gate");
        *open = true;
        self.ready.notify_all();
    }

    pub fn wait(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut open = self.open.lock().expect("lock gate");
        while !*open {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let (next_open, wait_result) = self
                .ready
                .wait_timeout(open, remaining)
                .expect("wait on gate condvar");
            open = next_open;
            if wait_result.timed_out() {
                break;
            }
        }
        *open
    }
}

impl Default for Gate {
    fn default() -> Self {
        Self::new_closed()
    }
}

/// Poll a shared `Mutex<Option<T>>` until populated or timeout.
pub fn wait_for_mutex_value<T: Clone>(
    value: &Arc<Mutex<Option<T>>>,
    timeout: Duration,
    poll_interval: Duration,
) -> Option<T> {
    let mut result = None;
    let found = wait_until(timeout, poll_interval, || {
        let current = value.lock().expect("lock mutex").clone();
        if current.is_some() {
            result = current;
            true
        } else {
            false
        }
    });
    if found { result } else { None }
}

/// Return a PID that is deterministically expected to be non-running on this host.
pub fn deterministic_non_running_pid() -> u32 {
    const MAX_SAFE_PID: u32 = i32::MAX as u32;
    for offset in 0..=1024 {
        let candidate = MAX_SAFE_PID - offset;
        if ralph::lock::pid_is_running(candidate) == Some(false) {
            return candidate;
        }
    }

    panic!("failed to find a deterministic non-running PID candidate");
}
