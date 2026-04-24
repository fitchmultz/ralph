//! Daemon log inspection and tailing implementation.
//!
//! Purpose:
//! - Daemon log inspection and tailing implementation.
//!
//! Responsibilities:
//! - Read and filter daemon log files with support for tail, follow, and JSON output.
//! - Parse timestamps and log levels from log lines for filtering.
//! - Handle log rotation and file truncation during follow mode.
//! - Provide machine-readable JSON output for integration with other tools.
//!
//! Not handled here:
//! - Log file creation and writing (handled by daemon process stdio redirection).
//! - Log rotation policy (handled externally).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Log files use RFC3339 timestamps at the start of lines when present.
//! - Log levels are detected via common token patterns (INFO, WARN, ERROR, etc.).
//! - Follow mode handles file truncation by resetting to the beginning.

use crate::cli::daemon::DaemonLogsArgs;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::mpsc;
use time::OffsetDateTime;

pub const DAEMON_LOG_FILE_NAME: &str = "daemon.log";

/// Output schema for `--json` daemon log mode.
#[derive(Debug, Serialize)]
pub(super) struct LogLineOutput {
    line_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<String>,
    line: String,
}

/// Internal representation for log line processing.
#[derive(Debug)]
pub(super) struct LogTailRecord {
    line_number: u64,
    line: String,
}

struct LogFileWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
}

impl LogFileWatcher {
    fn new(log_file: &Path) -> Result<Self> {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

        let parent = log_file
            .parent()
            .ok_or_else(|| io::Error::other("daemon log file has no parent directory"))?;
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )
        .context("Create daemon log watcher")?;
        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .with_context(|| format!("Watch daemon log directory {}", parent.display()))?;
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    fn wait_for_change(&self) -> Result<()> {
        match self.rx.recv() {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(anyhow::Error::from(error)).context("Watch daemon log file"),
            Err(_) => Ok(()),
        }
    }
}

/// Inspect daemon logs with filtering and follow support.
pub fn logs(resolved: &crate::config::Resolved, args: DaemonLogsArgs) -> Result<()> {
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

pub(super) fn emit_tail_output(
    log_file: &Path,
    args: &DaemonLogsArgs,
    writer: &mut impl Write,
) -> Result<()> {
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

pub(super) fn follow_log_file(
    log_file: &Path,
    args: &DaemonLogsArgs,
    writer: &mut impl Write,
) -> Result<()> {
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
    let watcher = LogFileWatcher::new(log_file).ok();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                let metadata = match std::fs::metadata(log_file) {
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

                if let Some(ref watcher) = watcher {
                    watcher.wait_for_change()?;
                } else {
                    std::thread::park_timeout(std::time::Duration::from_millis(150));
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

pub(super) fn read_tail_records(log_file: &Path, tail: usize) -> Result<(Vec<LogTailRecord>, u64)> {
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

pub(super) fn should_emit(line: &str, args: &DaemonLogsArgs) -> bool {
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

pub(super) fn emit_output(
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

pub(super) fn flush_with_compat(writer: &mut impl Write) -> Result<()> {
    match writer.flush() {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub(super) fn write_with_compat(writer: &mut impl Write, bytes: &[u8]) -> Result<()> {
    match writer.write_all(bytes) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Parse a RFC3339 timestamp from a log line.
pub(super) fn parse_line_timestamp(line: &str) -> Option<OffsetDateTime> {
    line.split_whitespace()
        .take(8)
        .flat_map(normalize_token_for_timestamp)
        .find_map(|token| crate::timeutil::parse_rfc3339(&token).ok())
}

/// Extract log level from a log line.
pub(super) fn extract_level(line: &str) -> Option<&'static str> {
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

pub(super) fn normalize_token(raw: &str) -> Vec<String> {
    let trimmed = raw
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .trim_end_matches(|c: char| !c.is_ascii_alphanumeric());

    if !trimmed.is_empty() {
        vec![trimmed.to_lowercase()]
    } else {
        vec![]
    }
}

pub(super) fn normalize_token_for_timestamp(raw: &str) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
