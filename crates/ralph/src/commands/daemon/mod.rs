//! Daemon command implementation for background service management.
//!
//! Responsibilities:
//! - Start/stop/status/logging and state management for a background Ralph daemon process.
//! - Manage daemon state and lock files.
//! - Run the continuous execution loop in daemon mode.
//! - Tail and follow daemon logs with filtering and machine-readable output.
//!
//! Not handled here:
//! - Windows service management (Unix-only implementation).
//! - Queue mutations (handled by `crate::queue`).
//! - CLI parsing and argument validation (handled in `crate::cli::daemon`).
//!
//! Invariants/assumptions:
//! - Daemon uses a dedicated lock at `.ralph/cache/daemon.lock`.
//! - Daemon state is stored at `.ralph/cache/daemon.json`.
//! - The serve command is internal and should not be called directly by users.
//! - Log output can vary; filtering falls back to permissive matching behavior.

use crate::cli::daemon::{DaemonLogsArgs, DaemonServeArgs, DaemonStartArgs};
use crate::config::Resolved;
use crate::lock::{self, PidLiveness, acquire_dir_lock};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use time::OffsetDateTime;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

const DAEMON_LOG_FILE_NAME: &str = "daemon.log";

/// Daemon state file name.
const DAEMON_STATE_FILE: &str = "daemon.json";
/// Daemon lock directory name (relative to .ralph/cache).
const DAEMON_LOCK_DIR: &str = "daemon.lock";

/// Output schema for `--json` daemon log mode.
#[derive(Debug, Serialize)]
struct LogLineOutput {
    line_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<String>,
    line: String,
}

/// Internal representation for log line processing.
#[derive(Debug)]
struct LogTailRecord {
    line_number: u64,
    line: String,
}

/// Daemon state persisted to disk.
#[derive(Debug, Serialize, Deserialize)]
struct DaemonState {
    /// Schema version for future compatibility.
    version: u32,
    /// Process ID of the daemon.
    pid: u32,
    /// ISO 8601 timestamp when the daemon started.
    started_at: String,
    /// Repository root path.
    repo_root: String,
    /// Full command line of the daemon process.
    command: String,
}

/// Start the daemon as a background process.
pub fn start(resolved: &Resolved, args: DaemonStartArgs) -> Result<()> {
    #[cfg(unix)]
    {
        let cache_dir = resolved.repo_root.join(".ralph/cache");
        let daemon_lock_dir = cache_dir.join(DAEMON_LOCK_DIR);

        // Check if daemon is already running
        if let Some(state) = get_daemon_state(&cache_dir)? {
            match daemon_pid_liveness(state.pid) {
                PidLiveness::Running => {
                    bail!(
                        "Daemon is already running (PID: {}). Use `ralph daemon stop` to stop it.",
                        state.pid
                    );
                }
                PidLiveness::Indeterminate => {
                    bail!(
                        "Daemon PID {} liveness is indeterminate. \
                         Preserving state/lock to prevent concurrent supervisors. \
                         {}",
                        state.pid,
                        manual_daemon_cleanup_instructions(&cache_dir)
                    );
                }
                PidLiveness::NotRunning => {
                    log::warn!("Removing stale daemon state file");
                    let state_path = cache_dir.join(DAEMON_STATE_FILE);
                    if let Err(e) = fs::remove_file(&state_path) {
                        log::debug!(
                            "Failed to remove stale daemon state file {}: {}",
                            state_path.display(),
                            e
                        );
                    }
                }
            }
        }

        // Try to acquire the daemon lock to ensure no other daemon is starting
        let _lock = match acquire_dir_lock(&daemon_lock_dir, "daemon-start", false) {
            Ok(lock) => lock,
            Err(e) => {
                bail!(
                    "Failed to acquire daemon lock: {}. Another daemon may be starting.",
                    e
                );
            }
        };

        // Build the serve command
        let exe = std::env::current_exe().context("Failed to get current executable path")?;
        let mut command = std::process::Command::new(&exe);
        command
            .arg("daemon")
            .arg("serve")
            .arg("--empty-poll-ms")
            .arg(args.empty_poll_ms.to_string())
            .arg("--wait-poll-ms")
            .arg(args.wait_poll_ms.to_string());

        if args.notify_when_unblocked {
            command.arg("--notify-when-unblocked");
        }

        // Set up stdio redirection
        let log_dir = resolved.repo_root.join(".ralph/logs");
        fs::create_dir_all(&log_dir).context("Failed to create log directory")?;
        let log_file = std::fs::File::create(log_dir.join(DAEMON_LOG_FILE_NAME))
            .context("Failed to create daemon log file")?;

        command
            .stdin(std::process::Stdio::null())
            .stdout(
                log_file
                    .try_clone()
                    .context("Failed to clone log file handle")?,
            )
            .stderr(log_file);

        // Detach from terminal on Unix
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }

        // Spawn the daemon process
        let child = command.spawn().context("Failed to spawn daemon process")?;
        let pid = child.id();

        if wait_for_daemon_state_pid(
            &cache_dir,
            pid,
            Duration::from_secs(2),
            Duration::from_millis(100),
        )? {
            println!("Daemon started successfully (PID: {})", pid);
            Ok(())
        } else {
            bail!("Daemon failed to start. Check .ralph/logs/daemon.log for details.");
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (resolved, args);
        bail!(
            "Daemon mode is only supported on Unix systems. Use `ralph run loop --continuous` in a terminal or configure a Windows service."
        );
    }
}

/// Stop the daemon gracefully.
pub fn stop(resolved: &Resolved) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");

    // Check if daemon is running
    let state = match get_daemon_state(&cache_dir)? {
        Some(state) => state,
        None => {
            println!("Daemon is not running");
            return Ok(());
        }
    };

    match daemon_pid_liveness(state.pid) {
        PidLiveness::NotRunning => {
            println!("Daemon is not running (removing stale state file)");
            let state_path = cache_dir.join(DAEMON_STATE_FILE);
            if let Err(e) = fs::remove_file(&state_path) {
                log::debug!(
                    "Failed to remove stale daemon state file {}: {}",
                    state_path.display(),
                    e
                );
            }
            let lock_path = cache_dir.join(DAEMON_LOCK_DIR);
            if let Err(e) = fs::remove_dir_all(&lock_path) {
                log::debug!(
                    "Failed to remove stale daemon lock dir {}: {}",
                    lock_path.display(),
                    e
                );
            }
            return Ok(());
        }
        PidLiveness::Indeterminate => {
            bail!(
                "Daemon PID {} liveness is indeterminate; preserving state/lock to avoid concurrent supervisors. \
                 {}",
                state.pid,
                manual_daemon_cleanup_instructions(&cache_dir)
            );
        }
        PidLiveness::Running => {}
    }

    // Create stop signal
    crate::signal::create_stop_signal(&cache_dir).context("Failed to create stop signal")?;
    println!("Stop signal sent to daemon (PID: {})", state.pid);

    // Wait up to 10 seconds for the daemon to exit
    println!("Waiting for daemon to stop...");
    for _ in 0..100 {
        std::thread::sleep(Duration::from_millis(100));
        if matches!(daemon_pid_liveness(state.pid), PidLiveness::NotRunning) {
            println!("Daemon stopped successfully");
            let state_path = cache_dir.join(DAEMON_STATE_FILE);
            if let Err(e) = fs::remove_file(&state_path) {
                log::debug!(
                    "Failed to remove daemon state file after stop {}: {}",
                    state_path.display(),
                    e
                );
            }
            return Ok(());
        }
    }

    // Daemon didn't stop in time
    bail!(
        "Daemon did not stop within 10 seconds. PID: {}. You may need to kill it manually with `kill -9 {}`",
        state.pid,
        state.pid
    );
}

/// Show daemon status.
pub fn status(resolved: &Resolved) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");

    match get_daemon_state(&cache_dir)? {
        Some(state) => {
            match daemon_pid_liveness(state.pid) {
                PidLiveness::Running => {
                    println!("Daemon is running");
                    println!("  PID: {}", state.pid);
                    println!("  Started: {}", state.started_at);
                    println!("  Command: {}", state.command);
                }
                PidLiveness::NotRunning => {
                    println!("Daemon is not running (stale state file detected)");
                    println!("  Last PID: {}", state.pid);
                    println!("  Last started: {}", state.started_at);
                    // Clean up stale state
                    let state_path = cache_dir.join(DAEMON_STATE_FILE);
                    if let Err(e) = fs::remove_file(&state_path) {
                        log::debug!(
                            "Failed to remove stale daemon state file {}: {}",
                            state_path.display(),
                            e
                        );
                    }
                    let lock_path = cache_dir.join(DAEMON_LOCK_DIR);
                    if let Err(e) = fs::remove_dir_all(&lock_path) {
                        log::debug!(
                            "Failed to remove stale daemon lock dir {}: {}",
                            lock_path.display(),
                            e
                        );
                    }
                }
                PidLiveness::Indeterminate => {
                    println!(
                        "Daemon PID liveness is indeterminate; preserving state/lock \
                         to avoid concurrent supervisors."
                    );
                    println!("  PID: {}", state.pid);
                    println!("  Started: {}", state.started_at);
                    println!("  Command: {}", state.command);
                    println!();
                    println!("{}", manual_daemon_cleanup_instructions(&cache_dir));
                }
            }
        }
        None => {
            println!("Daemon is not running");
        }
    }

    Ok(())
}

/// Inspect daemon logs with filtering and follow support.
pub fn logs(resolved: &Resolved, args: DaemonLogsArgs) -> Result<()> {
    let log_file = resolved
        .repo_root
        .join(".ralph")
        .join("logs")
        .join(DAEMON_LOG_FILE_NAME);

    if !log_file.exists() {
        if args.follow {
            bail!(
                "Daemon log file not found at {}. Start the daemon first with `ralph daemon start` or verify you are in the correct repository.\n",
                log_file.display()
            );
        }

        println!("No daemon log file found at {}.", log_file.display());
        println!("Start the daemon with `ralph daemon start` to generate logs.");
        return Ok(());
    }

    let mut out = io::BufWriter::new(io::stdout());
    if args.follow {
        follow_log_file(&log_file, &args, &mut out)?;
    } else {
        emit_tail_output(&log_file, &args, &mut out)?;
    }

    out.flush()?;
    Ok(())
}

/// Internal: Run the daemon serve loop.
/// This should not be called directly by users.
pub fn serve(resolved: &Resolved, args: DaemonServeArgs) -> Result<()> {
    let cache_dir = resolved.repo_root.join(".ralph/cache");
    let daemon_lock_dir = cache_dir.join(DAEMON_LOCK_DIR);

    // Acquire the daemon lock
    let _lock = acquire_dir_lock(&daemon_lock_dir, "daemon", false)
        .context("Failed to acquire daemon lock")?;

    // Write daemon state
    let state = DaemonState {
        version: 1,
        pid: std::process::id(),
        started_at: crate::timeutil::now_utc_rfc3339()?,
        repo_root: resolved.repo_root.display().to_string(),
        command: std::env::args().collect::<Vec<_>>().join(" "),
    };
    write_daemon_state(&cache_dir, &state)?;

    log::info!(
        "Daemon started (PID: {}, empty_poll={}ms, wait_poll={}ms)",
        state.pid,
        args.empty_poll_ms,
        args.wait_poll_ms
    );

    // Run the continuous execution loop
    let result = crate::commands::run::run_loop(
        resolved,
        crate::commands::run::RunLoopOptions {
            max_tasks: 0, // No limit in daemon mode
            agent_overrides: crate::agent::AgentOverrides::default(),
            force: true, // Force mode for unattended operation
            auto_resume: false,
            starting_completed: 0,
            non_interactive: true,
            parallel_workers: None,
            wait_when_blocked: true,
            wait_poll_ms: args.wait_poll_ms,
            wait_timeout_seconds: 0, // No timeout in daemon mode
            notify_when_unblocked: args.notify_when_unblocked,
            wait_when_empty: true,
            empty_poll_ms: args.empty_poll_ms,
        },
    );

    // Clean up state on exit
    log::info!("Daemon shutting down");
    let state_path = cache_dir.join(DAEMON_STATE_FILE);
    if let Err(e) = fs::remove_file(&state_path) {
        log::debug!(
            "Failed to remove daemon state file on shutdown {}: {}",
            state_path.display(),
            e
        );
    }

    result
}

fn emit_tail_output(log_file: &Path, args: &DaemonLogsArgs, writer: &mut impl Write) -> Result<()> {
    let (records, _) = read_tail_records(log_file, args.tail)?;
    for record in records {
        let ts = parse_line_timestamp(&record.line);
        let level = extract_level(&record.line);
        if should_emit(&record.line, args) {
            emit_output(
                writer,
                &record.line,
                record.line_number,
                args.json,
                ts.as_ref(),
                level,
            )?;
        }
    }
    Ok(())
}

fn follow_log_file(log_file: &Path, args: &DaemonLogsArgs, writer: &mut impl Write) -> Result<()> {
    let (seed_records, last_line) = read_tail_records(log_file, args.tail)?;
    let mut line_number = last_line;

    for record in seed_records {
        let ts = parse_line_timestamp(&record.line);
        let level = extract_level(&record.line);
        if should_emit(&record.line, args) {
            emit_output(
                writer,
                &record.line,
                record.line_number,
                args.json,
                ts.as_ref(),
                level,
            )?;
        }
    }

    let mut file = OpenOptions::new()
        .read(true)
        .open(log_file)
        .context("Open daemon log file")?;
    let mut reader = BufReader::new(file);
    let mut cursor = reader.seek(SeekFrom::End(0))?;

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                std::thread::sleep(Duration::from_millis(150));

                let metadata = match fs::metadata(log_file) {
                    Ok(meta) => meta,
                    Err(err) => {
                        if err.kind() == io::ErrorKind::NotFound {
                            break;
                        }
                        return Err(err).context("Read daemon log file metadata")?;
                    }
                };

                if metadata.len() < cursor {
                    file = OpenOptions::new().read(true).open(log_file)?;
                    reader = BufReader::new(file);
                    cursor = 0;
                    line_number = 0;
                }

                reader.seek(SeekFrom::Start(cursor))?;
            }
            Ok(_) => {
                cursor += line.len() as u64;
                line_number += 1;
                if should_emit(&line, args) {
                    let ts = parse_line_timestamp(&line);
                    let level = extract_level(&line);
                    emit_output(writer, &line, line_number, args.json, ts.as_ref(), level)?;
                }
            }
            Err(err) => {
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err).context("Failed to read daemon log line while following")?;
            }
        }
    }

    Ok(())
}

fn read_tail_records(log_file: &Path, tail: usize) -> Result<(Vec<LogTailRecord>, u64)> {
    let file = OpenOptions::new()
        .read(true)
        .open(log_file)
        .context("Open daemon log file")?;
    let mut reader = BufReader::new(file);
    let mut line_number = 0_u64;
    let mut lines: VecDeque<LogTailRecord> = VecDeque::new();

    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .context("Read daemon log line")?;
        if n == 0 {
            break;
        }
        line_number += 1;

        lines.push_back(LogTailRecord {
            line_number,
            line: line.clone(),
        });

        if lines.len() > tail {
            lines.pop_front();
        }
    }

    Ok((Vec::from(lines), line_number))
}

fn should_emit(line: &str, args: &DaemonLogsArgs) -> bool {
    if let Some(since) = args.since.as_ref() {
        let parsed = parse_line_timestamp(line);
        if parsed.is_none() || parsed.unwrap() < *since {
            return false;
        }
    }

    if let Some(level_filter) = args.level.as_deref() {
        let observed = extract_level(line);
        if observed != Some(level_filter) {
            return false;
        }
    }

    if let Some(contains) = args.contains.as_deref()
        && !line.contains(contains)
    {
        return false;
    }

    true
}

fn emit_output(
    writer: &mut impl Write,
    line: &str,
    line_number: u64,
    as_json: bool,
    seen_ts: Option<&OffsetDateTime>,
    seen_level: Option<&str>,
) -> Result<()> {
    if as_json {
        let payload = LogLineOutput {
            line_number,
            timestamp: seen_ts.map(|ts| ts.to_string()),
            level: seen_level.map(std::string::ToString::to_string),
            line: line.trim_end_matches(&['\r', '\n'][..]).to_string(),
        };
        let serialized =
            serde_json::to_string(&payload).context("Serialize daemon log JSON line")?;
        write_with_compat(writer, serialized.as_bytes())?;
        write_with_compat(writer, b"\n")?;
    } else {
        write_with_compat(writer, line.as_bytes())?;
    }

    flush_with_compat(writer)
}

fn flush_with_compat(writer: &mut impl Write) -> Result<()> {
    match writer.flush() {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn write_with_compat(writer: &mut impl Write, bytes: &[u8]) -> Result<()> {
    match writer.write_all(bytes) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Parse a RFC3339 timestamp from a log line.
fn parse_line_timestamp(line: &str) -> Option<OffsetDateTime> {
    line.split_whitespace()
        .take(8)
        .flat_map(normalize_token_for_timestamp)
        .find_map(|token| crate::timeutil::parse_rfc3339(&token).ok())
}

/// Extract log level from a log line.
fn extract_level(line: &str) -> Option<&'static str> {
    const LEVELS: &[(&str, &str)] = &[
        ("trace", "trace"),
        ("debug", "debug"),
        ("info", "info"),
        ("warn", "warn"),
        ("warning", "warn"),
        ("error", "error"),
        ("fatal", "fatal"),
        ("critical", "critical"),
    ];

    for token in line.split_whitespace().take(12).map(normalize_token) {
        for token in token {
            if let Some((_, level)) = LEVELS.iter().find(|(value, _)| *value == token.as_str()) {
                return Some(level);
            }
        }
    }

    None
}

fn normalize_token(raw: &str) -> Vec<String> {
    let trimmed = raw
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .trim_end_matches(|c: char| !c.is_ascii_alphanumeric());

    if !trimmed.is_empty() {
        vec![trimmed.to_lowercase()]
    } else {
        vec![]
    }
}

fn normalize_token_for_timestamp(raw: &str) -> Vec<String> {
    let trimmed = raw
        .trim_start_matches(|c: char| {
            !c.is_ascii_alphanumeric()
                && c != '-'
                && c != '+'
                && c != ':'
                && c != '.'
                && c != 'T'
                && c != 'Z'
                && c != 'z'
        })
        .trim_end_matches(|c: char| {
            !c.is_ascii_alphanumeric()
                && c != '-'
                && c != '+'
                && c != ':'
                && c != '.'
                && c != 'T'
                && c != 'Z'
                && c != 'z'
        });

    if !trimmed.is_empty() {
        vec![trimmed.to_lowercase()]
    } else {
        vec![]
    }
}

/// Read daemon state from disk.
fn get_daemon_state(cache_dir: &Path) -> Result<Option<DaemonState>> {
    let path = cache_dir.join(DAEMON_STATE_FILE);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read daemon state from {}", path.display()))?;

    let state: DaemonState = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse daemon state from {}", path.display()))?;

    Ok(Some(state))
}

/// Write daemon state to disk atomically.
fn write_daemon_state(cache_dir: &Path, state: &DaemonState) -> Result<()> {
    let path = cache_dir.join(DAEMON_STATE_FILE);
    let content =
        serde_json::to_string_pretty(state).context("Failed to serialize daemon state")?;
    crate::fsutil::write_atomic(&path, content.as_bytes())
        .with_context(|| format!("Failed to write daemon state to {}", path.display()))?;
    Ok(())
}

/// Poll daemon state until it matches `pid` or a timeout elapses.
fn wait_for_daemon_state_pid(
    cache_dir: &Path,
    pid: u32,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<bool> {
    let poll_interval = poll_interval.max(Duration::from_millis(1));
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(state) = get_daemon_state(cache_dir)?
            && state.pid == pid
        {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(poll_interval);
    }
}

/// Check PID liveness for daemon processes.
///
/// Returns tri-state liveness result to distinguish between definitive
/// running/not-running states and indeterminate cases.
fn daemon_pid_liveness(pid: u32) -> PidLiveness {
    lock::pid_liveness(pid)
}

/// Render manual cleanup instructions for stale/indeterminate daemon state.
///
/// This intentionally avoids suggesting `--force` because daemon subcommands
/// do not provide a force flag.
fn manual_daemon_cleanup_instructions(cache_dir: &Path) -> String {
    format!(
        "If you are certain the daemon is stopped, manually remove:\n  rm {}\n  rm -rf {}",
        cache_dir.join(DAEMON_STATE_FILE).display(),
        cache_dir.join(DAEMON_LOCK_DIR).display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn wait_for_daemon_state_pid_returns_true_when_state_appears() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        fs::create_dir_all(&cache_dir).expect("create cache dir");
        let expected_pid = 424_242_u32;

        let writer_cache_dir = cache_dir.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            let state = DaemonState {
                version: 1,
                pid: expected_pid,
                started_at: "2026-01-01T00:00:00Z".to_string(),
                repo_root: "/tmp/repo".to_string(),
                command: "ralph daemon serve".to_string(),
            };
            write_daemon_state(&writer_cache_dir, &state).expect("write daemon state");
        });

        let ready = wait_for_daemon_state_pid(
            &cache_dir,
            expected_pid,
            Duration::from_secs(1),
            Duration::from_millis(10),
        )
        .expect("poll daemon state");
        writer.join().expect("join writer thread");
        assert!(ready, "expected daemon state to appear before timeout");
    }

    #[test]
    fn wait_for_daemon_state_pid_returns_false_on_timeout() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        fs::create_dir_all(&cache_dir).expect("create cache dir");

        let ready = wait_for_daemon_state_pid(
            &cache_dir,
            123_456_u32,
            Duration::from_millis(100),
            Duration::from_millis(10),
        )
        .expect("poll daemon state");
        assert!(!ready, "expected timeout when daemon state is absent");
    }

    #[test]
    fn parse_line_timestamp_supports_rfc3339_prefixes() {
        let ts = parse_line_timestamp("2026-02-12T12:00:00Z INFO start");
        assert!(ts.is_some());
        assert_eq!(
            ts.expect("timestamp").to_string(),
            "2026-02-12 12:00:00.0 +00:00:00"
        );
    }

    #[test]
    fn extract_level_recognizes_level_tokens() {
        assert_eq!(extract_level("INFO service start"), Some("info"));
        assert_eq!(extract_level("warn: queue stalled"), Some("warn"));
        assert_eq!(extract_level("unknown message"), None);
    }

    #[test]
    fn manual_cleanup_instructions_include_state_and_lock_paths() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        let instructions = manual_daemon_cleanup_instructions(&cache_dir);

        assert!(instructions.contains(&format!(
            "rm {}",
            cache_dir.join(DAEMON_STATE_FILE).display()
        )));
        assert!(instructions.contains(&format!(
            "rm -rf {}",
            cache_dir.join(DAEMON_LOCK_DIR).display()
        )));
    }

    #[test]
    fn manual_cleanup_instructions_do_not_reference_force_flag() {
        let temp = TempDir::new().expect("create temp dir");
        let cache_dir = temp.path().join(".ralph/cache");
        let instructions = manual_daemon_cleanup_instructions(&cache_dir);

        assert!(
            !instructions.contains("--force"),
            "daemon cleanup instructions must not mention nonexistent --force flag"
        );
    }

    #[test]
    fn emit_output_non_json_preserves_line() {
        let mut output = Vec::new();
        emit_output(&mut output, "line one\n", 12, false, None, None).expect("emit line");

        assert_eq!(String::from_utf8_lossy(&output), "line one\n");
    }

    #[test]
    fn emit_output_json_minimal_fields() {
        let line = "2026-02-12T12:00:00Z INFO test\n";
        let parsed_ts = parse_line_timestamp(line);
        let parsed_level = extract_level(line);
        let mut output = Vec::new();

        emit_output(
            &mut output,
            line,
            42,
            true,
            parsed_ts.as_ref(),
            parsed_level,
        )
        .expect("emit json");

        let emitted = String::from_utf8_lossy(&output);
        assert!(emitted.contains("\"line_number\":42"));
        assert!(emitted.contains("\"timestamp\""));
        assert!(emitted.contains("\"level\":\"info\""));
    }
}
