//! Integration tests for daemon mode (`ralph daemon`).
//!
//! Responsibilities:
//! - Test that daemon commands exist and have proper help output
//! - Test stale state detection and cleanup
//!
//! Not handled here:
//! - Full daemon lifecycle tests (requires subprocess spawning which is flaky in test env)
//! - Continuous mode logic (see run_loop_continuous_test.rs)
//! - Windows service management (daemon is Unix-only)

mod test_support;

#[cfg(unix)]
mod unix_tests {
    use super::test_support;
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Write};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    fn write_log_file(dir: &Path, lines: &[&str]) {
        let log_dir = dir.join(".ralph/logs");
        std::fs::create_dir_all(&log_dir).expect("create log dir");
        let log_file = log_dir.join("daemon.log");
        let mut file = std::fs::File::create(&log_file).expect("create daemon log");
        for line in lines {
            writeln!(file, "{line}").expect("write daemon log line");
        }
    }

    fn command_output(
        args: &[&str],
        current_dir: &Path,
    ) -> (std::process::ExitStatus, String, String) {
        let ralph = test_support::ralph_bin();
        let output = Command::new(&ralph)
            .args(args)
            .current_dir(current_dir)
            .output()
            .expect("failed to run ralph daemon logs");

        (
            output.status,
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )
    }

    /// Test that daemon --help works.
    #[test]
    fn daemon_help_works() {
        let ralph = test_support::ralph_bin();

        let help = Command::new(&ralph)
            .arg("daemon")
            .arg("--help")
            .output()
            .expect("Failed to run ralph daemon --help");

        assert!(help.status.success(), "daemon --help should succeed");
        let stdout = String::from_utf8_lossy(&help.stdout);
        assert!(
            stdout.contains("start"),
            "Help should mention start subcommand"
        );
        assert!(
            stdout.contains("stop"),
            "Help should mention stop subcommand"
        );
        assert!(
            stdout.contains("status"),
            "Help should mention status subcommand"
        );
        assert!(
            stdout.contains("logs"),
            "Help should mention logs subcommand"
        );
    }

    /// Test that daemon logs can filter and tail by contains and level.
    #[test]
    fn daemon_logs_shows_recent_with_filters() {
        let dir = test_support::temp_dir_outside_repo();
        let dir_path = dir.path();

        test_support::git_init(dir_path).expect("git init");

        write_log_file(
            dir_path,
            &[
                "2026-02-10T10:00:00Z INFO daemon bootstrap complete",
                "2026-02-10T10:01:00Z ERROR webhook notification failed",
                "2026-02-10T10:02:00Z WARN queue stalled while waiting",
            ],
        );

        let (status, stdout, _stderr) = command_output(
            &["daemon", "logs", "--tail", "2", "--contains", "queue"],
            dir_path,
        );

        assert!(status.success());
        assert_eq!(stdout.matches("queue").count(), 1);
        assert!(stdout.contains("WARN queue stalled while waiting"));
        assert!(!stdout.contains("bootstrap"));
    }

    /// Test JSON output shape and filtering by level.
    #[test]
    fn daemon_logs_json_output() {
        let dir = test_support::temp_dir_outside_repo();
        let dir_path = dir.path();

        test_support::git_init(dir_path).expect("git init");

        write_log_file(
            dir_path,
            &[
                "2026-02-10T10:00:00Z INFO daemon bootstrap complete",
                "2026-02-10T10:01:00Z ERROR webhook notification failed",
                "2026-02-10T10:02:00Z DEBUG webhook retry scheduled",
            ],
        );

        let (status, stdout, _stderr) = command_output(
            &[
                "daemon", "logs", "--json", "--tail", "2", "--level", "error",
            ],
            dir_path,
        );

        assert!(status.success());
        let lines: Vec<Value> = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("parse JSON line"))
            .collect();

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["line_number"].as_u64(), Some(2));
        assert_eq!(lines[0]["level"], "error");
        assert_eq!(
            lines[0]["line"].as_str().expect("json line"),
            "2026-02-10T10:01:00Z ERROR webhook notification failed"
        );
    }

    /// Test --since filtering and follow semantics.
    #[test]
    fn daemon_logs_since_filter() {
        let dir = test_support::temp_dir_outside_repo();
        let dir_path = dir.path();

        test_support::git_init(dir_path).expect("git init");

        write_log_file(
            dir_path,
            &[
                "2026-01-10T10:00:00Z INFO old event",
                "2026-02-10T10:00:00Z ERROR new event",
            ],
        );

        let (status, stdout, _stderr) = command_output(
            &[
                "daemon",
                "logs",
                "--since",
                "2026-02-01T00:00:00Z",
                "--contains",
                "event",
            ],
            dir_path,
        );
        assert!(status.success());
        assert!(stdout.contains("new event"));
        assert!(!stdout.contains("old event"));
    }

    /// Test missing daemon log file guidance and follow error behavior.
    #[test]
    fn daemon_logs_missing_file_guidance() {
        let dir = test_support::temp_dir_outside_repo();
        let dir_path = dir.path();

        test_support::git_init(dir_path).expect("git init");

        let (status, stdout, stderr) = command_output(&["daemon", "logs"], dir_path);
        assert!(status.success());
        assert!(
            stdout.contains("No daemon log file found")
                || stderr.contains("No daemon log file found")
        );

        let (status_follow, _stdout_follow, stderr_follow) =
            command_output(&["daemon", "logs", "--follow"], dir_path);
        assert!(!status_follow.success());
        assert!(
            stderr_follow.contains("log file not found")
                || stderr_follow.contains("Daemon log file not found")
        );
    }

    /// Test follow mode emits lines as they are appended.
    #[test]
    fn daemon_logs_follow_appends_lines() {
        let dir = test_support::temp_dir_outside_repo();
        let dir_path = dir.path();
        test_support::git_init(dir_path).expect("git init");

        write_log_file(dir_path, &["2026-02-10T10:00:00Z INFO initial line"]);

        let log_path = dir_path.join(".ralph/logs/daemon.log");
        let ralph = test_support::ralph_bin();
        let mut child = Command::new(&ralph)
            .arg("daemon")
            .arg("logs")
            .arg("--follow")
            .arg("--tail")
            .arg("1")
            .current_dir(dir_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemon logs follow");

        let child_stdout = child.stdout.take().expect("capture follow stdout");
        let output = Arc::new(Mutex::new(String::new()));
        let output_reader = Arc::clone(&output);
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(child_stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if !line.is_empty() {
                            output_reader.lock().expect("lock output").push_str(&line);
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut appended = std::fs::OpenOptions::new()
            .append(true)
            .open(&log_path)
            .expect("open log file to append");
        appended
            .write_all(b"2026-02-10T10:02:00Z WARN follow alert\n")
            .expect("append log line");
        appended.flush().expect("flush append");

        let saw_append =
            test_support::wait_until(Duration::from_secs(3), Duration::from_millis(25), || {
                output.lock().expect("lock output").contains("follow alert")
            });
        assert!(saw_append, "follow output did not include appended line");

        child.kill().expect("kill follow command");
        let _ = child.wait().expect("wait follow command");
        reader.join().expect("join reader thread");
    }

    /// Test that daemon start --help works.
    #[test]
    fn daemon_start_help_works() {
        let ralph = test_support::ralph_bin();

        let help = Command::new(&ralph)
            .arg("daemon")
            .arg("start")
            .arg("--help")
            .output()
            .expect("Failed to run ralph daemon start --help");

        assert!(help.status.success(), "daemon start --help should succeed");
        let stdout = String::from_utf8_lossy(&help.stdout);
        assert!(
            stdout.contains("empty-poll-ms"),
            "Help should mention empty-poll-ms flag"
        );
    }

    /// Test that status handles stale state files correctly.
    #[test]
    fn daemon_status_handles_stale_state() {
        let dir = test_support::temp_dir_outside_repo();
        let ralph = test_support::ralph_bin();
        let dir_path = dir.path();

        // Initialize git repo
        test_support::git_init(dir_path).expect("git init");

        // Initialize ralph
        let init = Command::new(&ralph)
            .arg("init")
            .arg("--force")
            .arg("--non-interactive")
            .current_dir(dir_path)
            .output()
            .expect("Failed to run ralph init");
        assert!(init.status.success());

        // Create a fake stale daemon state file with a non-existent PID
        let cache_dir = dir_path.join(".ralph/cache");
        std::fs::create_dir_all(&cache_dir).expect("create cache dir");
        let fake_state = serde_json::json!({
            "version": 1,
            "pid": test_support::deterministic_non_running_pid(),
            "started_at": "2026-01-01T00:00:00Z",
            "repo_root": dir_path.to_string_lossy().to_string(),
            "command": "ralph daemon serve"
        });
        std::fs::write(
            cache_dir.join("daemon.json"),
            serde_json::to_string_pretty(&fake_state).unwrap(),
        )
        .expect("write fake state");

        // Check status - should detect stale state
        let status = Command::new(&ralph)
            .arg("daemon")
            .arg("status")
            .current_dir(dir_path)
            .output()
            .expect("Failed to run ralph daemon status");

        let status_stdout = String::from_utf8_lossy(&status.stdout);
        assert!(
            status_stdout.contains("not running") || status_stdout.contains("stale"),
            "Status should report not running or stale: {}",
            status_stdout
        );
        let state_path = cache_dir.join("daemon.json");
        assert!(
            test_support::wait_until(
                std::time::Duration::from_secs(5),
                std::time::Duration::from_millis(25),
                || !state_path.exists(),
            ),
            "stale daemon state file should be removed by daemon status"
        );
    }
}
