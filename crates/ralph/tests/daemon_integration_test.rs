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
    use std::process::Command;

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
                std::time::Duration::from_secs(1),
                std::time::Duration::from_millis(10),
                || !state_path.exists(),
            ),
            "stale daemon state file should be removed by daemon status"
        );
    }
}
