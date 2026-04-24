//! Shared logging helpers for run-command tests.
//!
//! Purpose:
//! - Shared logging helpers for run-command tests.
//!
//! Responsibilities:
//! - Install a deterministic in-memory logger for tests that assert warnings.
//! - Provide helpers for draining captured log lines between assertions.
//!
//! Not handled here:
//! - Test fixtures unrelated to log capture.
//! - Production logging configuration.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests may race with a preinstalled logger, so logger installation is best-effort.
//! - Captured logs remain process-global and must be drained between assertions.

use log::{LevelFilter, Log, Metadata, Record};
use std::sync::{Mutex, OnceLock};

pub(crate) struct TestLogger;

static LOGGER: TestLogger = TestLogger;
static LOGGER_STATE: OnceLock<LoggerState> = OnceLock::new();
static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoggerState {
    TestLogger,
    OtherLogger,
}

impl Log for TestLogger {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        let logs = LOGS.get_or_init(|| Mutex::new(Vec::new()));
        let mut guard = logs.lock().expect("log mutex");
        guard.push(record.args().to_string());
    }

    fn flush(&self) {}
}

fn init_logger() -> (LoggerState, &'static Mutex<Vec<String>>) {
    let state = *LOGGER_STATE.get_or_init(|| {
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(LevelFilter::Warn);
            LoggerState::TestLogger
        } else {
            LoggerState::OtherLogger
        }
    });
    (state, LOGS.get_or_init(|| Mutex::new(Vec::new())))
}

pub(crate) fn take_logs() -> (LoggerState, Vec<String>) {
    let (state, logs) = init_logger();
    let mut guard = logs.lock().expect("log mutex");
    let drained = guard.drain(..).collect::<Vec<_>>();
    (state, drained)
}
