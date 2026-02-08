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

#[cfg(unix)]
mod unix_tests {
    use std::process::Command;

    fn ralph_bin() -> std::path::PathBuf {
        if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
            return std::path::PathBuf::from(path);
        }

        let exe = std::env::current_exe().expect("resolve current test executable path");
        let exe_dir = exe
            .parent()
            .expect("test executable should have a parent directory");
        let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
            exe_dir
                .parent()
                .expect("deps directory should have a parent directory")
        } else {
            exe_dir
        };

        let bin_name = "ralph";
        let candidate = profile_dir.join(bin_name);
        if candidate.exists() {
            return candidate;
        }

        panic!(
            "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
            candidate.display()
        );
    }

    fn temp_dir_outside_repo() -> tempfile::TempDir {
        use std::path::PathBuf;

        let candidates: [PathBuf; 2] = [std::env::temp_dir(), PathBuf::from("/tmp")];

        for candidate in candidates {
            if candidate.as_os_str().is_empty() {
                continue;
            }
            if std::fs::metadata(&candidate).is_ok() {
                return tempfile::TempDir::new_in(&candidate).expect("create temp dir");
            }
        }

        panic!("Could not find suitable temp directory");
    }

    fn git_init(dir: &std::path::Path) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(["init", "--quiet"])
            .status()
            .expect("run git init");
        assert!(status.success(), "git init failed");

        let gitignore_path = dir.join(".gitignore");
        std::fs::write(
            &gitignore_path,
            ".ralph/lock\n.ralph/cache/\n.ralph/logs/\n",
        )
        .expect("write .gitignore");

        Command::new("git")
            .current_dir(dir)
            .args(["add", ".gitignore"])
            .status()
            .expect("git add");

        Command::new("git")
            .current_dir(dir)
            .args(["commit", "--quiet", "-m", "add gitignore"])
            .status()
            .expect("git commit");
    }

    /// Test that daemon --help works.
    #[test]
    fn daemon_help_works() {
        let ralph = ralph_bin();

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
        let ralph = ralph_bin();

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
        let dir = temp_dir_outside_repo();
        let ralph = ralph_bin();
        let dir_path = dir.path();

        // Initialize git repo
        git_init(dir_path);

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
            "pid": 99999, // Non-existent PID
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
    }
}
