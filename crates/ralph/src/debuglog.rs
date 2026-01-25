//! Debug logging for raw, unredacted supervisor and runner output.

use anyhow::{anyhow, bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug)]
pub struct DebugLog {
    file: Mutex<std::fs::File>,
}

impl DebugLog {
    pub fn new(repo_root: &Path) -> Result<Self> {
        let logs_dir = repo_root.join(".ralph").join("logs");
        if logs_dir.exists() && !logs_dir.is_dir() {
            bail!(
                "debug logs path exists and is not a directory: {}",
                logs_dir.display()
            );
        }
        fs::create_dir_all(&logs_dir)
            .with_context(|| format!("create debug logs directory: {}", logs_dir.display()))?;
        let path = logs_dir.join("debug.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("open debug log file: {}", path.display()))?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub fn write(&self, text: &str) -> Result<()> {
        let mut guard = self
            .file
            .lock()
            .map_err(|_| anyhow!("lock debug log file"))?;
        guard
            .write_all(text.as_bytes())
            .context("write debug log")?;
        guard.flush().context("flush debug log")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugStream {
    Stdout,
    Stderr,
}

static DEBUG_LOG: OnceLock<Mutex<Option<Arc<DebugLog>>>> = OnceLock::new();

fn debug_log_state() -> &'static Mutex<Option<Arc<DebugLog>>> {
    DEBUG_LOG.get_or_init(|| Mutex::new(None))
}

pub fn enable(repo_root: &Path) -> Result<()> {
    let log = Arc::new(DebugLog::new(repo_root)?);
    let mut guard = debug_log_state()
        .lock()
        .map_err(|_| anyhow!("lock debug log state"))?;
    if guard.is_none() {
        *guard = Some(log);
    }
    Ok(())
}

pub fn with_debug_log<F>(mut f: F)
where
    F: FnMut(&DebugLog),
{
    let guard = match debug_log_state().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if let Some(log) = guard.as_ref() {
        f(log);
    }
}

pub fn write_log_record(record: &log::Record<'_>) {
    with_debug_log(|log| {
        let mut line = format!(
            "[LOG {} {}] {}",
            record.level(),
            record.target(),
            record.args()
        );
        if !line.ends_with('\n') {
            line.push('\n');
        }
        let _ = log.write(&line);
    });
}

pub fn write_runner_chunk(stream: DebugStream, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    with_debug_log(|log| {
        let header = match stream {
            DebugStream::Stdout => "[RUNNER STDOUT]\n",
            DebugStream::Stderr => "[RUNNER STDERR]\n",
        };
        let _ = log.write(header);
        let _ = log.write(chunk);
    });
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    if let Some(state) = DEBUG_LOG.get() {
        if let Ok(mut guard) = state.lock() {
            *guard = None;
        }
    }
}

#[cfg(test)]
pub(crate) fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::{
        enable, reset_for_tests, test_lock, write_log_record, write_runner_chunk, DebugStream,
    };
    use log::Record;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn enable_creates_log_file_and_writes() {
        let _guard = test_lock().lock().expect("debug log lock");
        reset_for_tests();
        let dir = tempdir().expect("tempdir");
        enable(dir.path()).expect("enable");

        let record = Record::builder()
            .level(log::Level::Info)
            .target("test")
            .args(format_args!("hello debug log"))
            .build();
        write_log_record(&record);
        write_runner_chunk(DebugStream::Stdout, "runner output\n");

        let debug_log = dir.path().join(".ralph/logs/debug.log");
        let contents = fs::read_to_string(&debug_log).expect("read log");
        assert!(
            contents.contains("hello debug log"),
            "log contents: {contents}"
        );
        assert!(
            contents.contains("[RUNNER STDOUT]"),
            "log contents: {contents}"
        );
        assert!(
            contents.contains("runner output\n"),
            "log contents: {contents}"
        );
        reset_for_tests();
    }

    #[test]
    fn enable_errors_when_logs_path_is_file() {
        let _guard = test_lock().lock().expect("debug log lock");
        reset_for_tests();
        let dir = tempdir().expect("tempdir");
        let ralph_dir = dir.path().join(".ralph");
        fs::create_dir_all(&ralph_dir).expect("mkdir");
        let logs_path = ralph_dir.join("logs");
        fs::write(&logs_path, "not a dir").expect("write logs file");

        let err = enable(dir.path()).expect_err("error");
        assert!(err.to_string().contains("debug logs path"));
        reset_for_tests();
    }

    #[test]
    fn write_noop_when_disabled() {
        let _guard = test_lock().lock().expect("debug log lock");
        reset_for_tests();
        let dir = tempdir().expect("tempdir");

        let record = Record::builder()
            .level(log::Level::Info)
            .target("test")
            .args(format_args!("no log"))
            .build();
        write_log_record(&record);
        write_runner_chunk(DebugStream::Stderr, "no runner\n");

        let debug_log = dir.path().join(".ralph/logs/debug.log");
        assert!(!debug_log.exists(), "debug log should not exist");
        reset_for_tests();
    }
}
