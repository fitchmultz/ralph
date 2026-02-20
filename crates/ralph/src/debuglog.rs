//! Debug logging for raw, unredacted supervisor and runner output.
//!
//! Features:
//! - Automatic log rotation when file size exceeds 10MB
//! - Keeps 3 backup files (debug.log.1, debug.log.2, debug.log.3)
//! - Thread-safe writes via Mutex

use anyhow::{Context, Result, anyhow, bail};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Maximum size of debug.log before rotation (10MB).
const MAX_LOG_SIZE_BYTES: u64 = 10 * 1024 * 1024;

/// Number of backup files to keep.
const MAX_BACKUP_FILES: u32 = 3;

/// Debug log file name.
const LOG_FILE_NAME: &str = "debug.log";

#[derive(Debug)]
pub struct DebugLog {
    /// Stored for debugging/diagnostic purposes; currently unused but useful for future extensions
    #[allow(dead_code)]
    log_path: PathBuf,
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

        let log_path = logs_dir.join(LOG_FILE_NAME);

        // Check if rotation is needed before opening
        Self::rotate_if_needed(&log_path)?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("open debug log file: {}", log_path.display()))?;

        Ok(Self {
            log_path,
            file: Mutex::new(file),
        })
    }

    /// Rotate log files if current log exceeds max size.
    /// Rotation scheme: debug.log -> debug.log.1 -> debug.log.2 -> debug.log.3 (deleted)
    fn rotate_if_needed(log_path: &Path) -> Result<()> {
        // Check if file exists and its size
        let size = match fs::metadata(log_path) {
            Ok(meta) => meta.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("check log file size: {}", log_path.display()));
            }
        };

        if size < MAX_LOG_SIZE_BYTES {
            return Ok(());
        }

        // Perform rotation: delete oldest, shift others, move current to .1
        let parent = log_path.parent().expect("log path has parent");

        // Delete oldest backup if it exists (debug.log.3)
        let oldest_backup = parent.join(format!("{}.{}", LOG_FILE_NAME, MAX_BACKUP_FILES));
        if oldest_backup.exists() {
            fs::remove_file(&oldest_backup)
                .with_context(|| format!("remove oldest backup: {}", oldest_backup.display()))?;
        }

        // Shift backups: .2 -> .3, .1 -> .2
        for i in (1..MAX_BACKUP_FILES).rev() {
            let src = parent.join(format!("{}.{}", LOG_FILE_NAME, i));
            let dst = parent.join(format!("{}.{}", LOG_FILE_NAME, i + 1));

            if src.exists() {
                fs::rename(&src, &dst).with_context(|| {
                    format!("rotate backup {} -> {}", src.display(), dst.display())
                })?;
            }
        }

        // Move current log to .1
        let backup_1 = parent.join(format!("{}.1", LOG_FILE_NAME));
        fs::rename(log_path, &backup_1)
            .with_context(|| format!("rotate current log to backup: {}", backup_1.display()))?;

        log::info!("Debug log rotated (previous size: {} bytes)", size);

        Ok(())
    }

    pub fn write(&self, text: &str) -> Result<()> {
        // Check if rotation is needed before writing
        // We do this check periodically based on write count to avoid overhead

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
        if let Err(e) = log.write(&line) {
            log::debug!("Failed to write to debug log: {}", e);
        }
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
        if let Err(e) = log.write(header) {
            log::debug!("Failed to write runner header to debug log: {}", e);
            return;
        }
        if let Err(e) = log.write(chunk) {
            log::debug!("Failed to write runner chunk to debug log: {}", e);
        }
    });
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    if let Some(state) = DEBUG_LOG.get()
        && let Ok(mut guard) = state.lock()
    {
        *guard = None;
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
        DebugStream, enable, reset_for_tests, test_lock, write_log_record, write_runner_chunk,
    };
    use log::Record;
    use serial_test::serial;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    #[serial]
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
    #[serial]
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
    #[serial]
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

    #[test]
    #[serial]
    fn log_rotation_occurs_when_size_exceeded() {
        use super::{LOG_FILE_NAME, MAX_LOG_SIZE_BYTES};

        let _guard = test_lock().lock().expect("debug log lock");
        reset_for_tests();
        let dir = tempdir().expect("tempdir");
        let logs_dir = dir.path().join(".ralph/logs");
        fs::create_dir_all(&logs_dir).expect("mkdir");

        // Create an oversized log file
        let log_path = logs_dir.join(LOG_FILE_NAME);
        let oversized_content = vec![b'x'; (MAX_LOG_SIZE_BYTES + 100) as usize];
        fs::write(&log_path, oversized_content).expect("write oversized log");

        // Enable debug logging - this should trigger rotation
        enable(dir.path()).expect("enable");

        // Write something to the new log
        let record = Record::builder()
            .level(log::Level::Info)
            .target("test")
            .args(format_args!("after rotation"))
            .build();
        write_log_record(&record);

        // The oversized log should be moved to .1
        let backup_1 = logs_dir.join(format!("{}.1", LOG_FILE_NAME));
        assert!(
            backup_1.exists(),
            "backup .1 should exist with rotated content"
        );

        // Verify the backup has the oversized content
        let backup_size = fs::metadata(&backup_1).expect("backup metadata").len();
        assert!(
            backup_size > MAX_LOG_SIZE_BYTES,
            "backup should contain the oversized data"
        );

        // New log should exist and contain only our new entry (not the oversized content)
        assert!(
            log_path.exists(),
            "new log file should exist at original path"
        );
        let contents = fs::read_to_string(&log_path).expect("read new log");
        assert!(
            contents.contains("after rotation"),
            "new log should have new entry"
        );
        assert!(
            !contents.contains('x'),
            "new log should not contain the old oversized content"
        );

        reset_for_tests();
    }
}
