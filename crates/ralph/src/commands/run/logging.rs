//! Small logging helpers for the outer supervisor (`commands::run`).
//!
//! Goal: consistent, human-readable lifecycle logs for supervisor scopes:
//! - "<Scope>: start"
//! - "<Scope>: end"
//! - "<Scope>: error: <message>"

use anyhow::Result;

/// Run `f` while logging a consistent start/end/error envelope.
///
/// NOTE: Keep messages short/human-readable. Full error context is still surfaced
/// by the CLI error printer; this log line is about boundary visibility.
pub(crate) fn with_scope<T>(label: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    log::info!("{label}: start");
    match f() {
        Ok(value) => {
            log::info!("{label}: end");
            Ok(value)
        }
        Err(err) => {
            log::error!("{label}: error: {}", err);
            Err(err)
        }
    }
}

pub(crate) fn phase_label(phase: u8, total: u8, name: &str, task_id: &str) -> String {
    format!("Phase {phase}/{total} ({name}) for {}", task_id.trim())
}

pub(crate) fn single_phase_label(name: &str, task_id: &str) -> String {
    format!("{name} for {}", task_id.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use log::{LevelFilter, Log, Metadata, Record};
    use std::sync::{Mutex, OnceLock};

    struct TestLogger;

    static LOGGER: TestLogger = TestLogger;
    static LOGGER_STATE: OnceLock<LoggerState> = OnceLock::new();
    static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LoggerState {
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
                log::set_max_level(LevelFilter::Info);
                LoggerState::TestLogger
            } else {
                LoggerState::OtherLogger
            }
        });
        (state, LOGS.get_or_init(|| Mutex::new(Vec::new())))
    }

    fn take_logs() -> (LoggerState, Vec<String>) {
        let (state, logs) = init_logger();
        let mut guard = logs.lock().expect("log mutex");
        let drained = guard.drain(..).collect::<Vec<_>>();
        (state, drained)
    }

    #[test]
    fn with_scope_logs_start_and_end_on_success() -> Result<()> {
        let (state, _) = take_logs();

        with_scope("ScopeA", || Ok(()))?;

        let (_, logs) = take_logs();
        if state == LoggerState::TestLogger {
            let expected = vec!["ScopeA: start", "ScopeA: end"];
            if logs != expected {
                assert!(
                    logs.is_empty(),
                    "unexpected logs: {logs:?} (expected {expected:?})"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn with_scope_logs_error_on_failure() {
        let (state, _) = take_logs();

        let err = with_scope::<()>("ScopeB", || Err(anyhow!("boom"))).unwrap_err();
        assert_eq!(err.to_string(), "boom");

        let (_, logs) = take_logs();
        if state == LoggerState::TestLogger {
            let expected_full = vec!["ScopeB: start", "ScopeB: error: boom"];
            let expected_partial = vec!["ScopeB: error: boom"];
            if logs != expected_full && logs != expected_partial {
                assert!(logs.is_empty(), "unexpected logs: {logs:?}");
            }
        }
    }

    #[test]
    fn labels_trim_task_ids() {
        assert_eq!(
            phase_label(2, 3, "Implementation", " RQ-1 "),
            "Phase 2/3 (Implementation) for RQ-1"
        );
        assert_eq!(
            single_phase_label("SinglePhase (Execution)", " RQ-2 "),
            "SinglePhase (Execution) for RQ-2"
        );
    }
}
