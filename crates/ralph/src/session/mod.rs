//! Session persistence for crash recovery.
//!
//! Responsibilities:
//! - Save, load, and clear session state to/from .ralph/cache/session.jsonc.
//! - Validate session state against current queue state.
//! - Provide session recovery detection and prompts.
//!
//! Not handled here:
//! - Session state definition (see crate::contracts::session).
//! - Task execution logic (see crate::commands::run).
//!
//! Invariants/assumptions:
//! - Session file is written atomically using fsutil::write_atomic.
//! - Session is considered stale if task no longer exists or is not Doing.
//! - Session timeout is checked before allowing resume.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::constants::paths::SESSION_FILENAME;
use crate::contracts::{QueueFile, SessionState, TaskStatus};
use crate::fsutil;
use crate::runutil::{ManagedCommand, TimeoutClass, execute_checked_command};
use crate::timeutil;

/// Get the path to the session file.
pub fn session_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(SESSION_FILENAME)
}

/// Check if a session file exists.
pub fn session_exists(cache_dir: &Path) -> bool {
    session_path(cache_dir).exists()
}

/// Save session state to disk.
pub fn save_session(cache_dir: &Path, session: &SessionState) -> Result<()> {
    let path = session_path(cache_dir);
    let json = serde_json::to_string_pretty(session).context("serialize session state")?;
    fsutil::write_atomic(&path, json.as_bytes()).context("write session file")?;
    log::debug!("Session saved: task_id={}", session.task_id);
    Ok(())
}

/// Load session state from disk.
pub fn load_session(cache_dir: &Path) -> Result<Option<SessionState>> {
    let path = session_path(cache_dir);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).context("read session file")?;
    let session: SessionState = serde_json::from_str(&content).context("parse session file")?;

    // Version check for forward compatibility
    if session.version > crate::contracts::SESSION_STATE_VERSION {
        log::warn!(
            "Session file version {} is newer than supported version {}. \
             Attempting to load anyway.",
            session.version,
            crate::contracts::SESSION_STATE_VERSION
        );
    }

    Ok(Some(session))
}

/// Clear (delete) the session file.
pub fn clear_session(cache_dir: &Path) -> Result<()> {
    let path = session_path(cache_dir);
    if path.exists() {
        std::fs::remove_file(&path).context("remove session file")?;
        log::debug!("Session cleared");
    }
    Ok(())
}

/// Increment the session's tasks_completed_in_loop counter and persist.
///
/// Returns Ok(()) on success, or an error if session load/save fails.
/// Logs a warning on failure but does not propagate the error to avoid
/// disrupting the run loop.
pub fn increment_session_progress(cache_dir: &Path) -> Result<()> {
    let mut session = match load_session(cache_dir)? {
        Some(s) => s,
        None => {
            log::debug!("No session to increment progress for");
            return Ok(());
        }
    };

    let now = crate::timeutil::now_utc_rfc3339_or_fallback();
    session.mark_task_complete(now);
    save_session(cache_dir, &session)
}

/// Result of session validation.
// Allow large enum variant because SessionState is naturally large (contains strings and phase
// settings) and boxing would add complexity to all usage sites without meaningful benefit.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionValidationResult {
    /// Session is valid and can be resumed.
    Valid(SessionState),
    /// No session file exists.
    NoSession,
    /// Session is stale (task completed, rejected, or no longer exists).
    Stale { reason: String },
    /// Session has timed out (older than threshold).
    Timeout { hours: u64, session: SessionState },
}

/// Internal helper that accepts an injected `now` for deterministic testing.
///
/// Compares `now` against the session's `last_updated_at` to detect timeouts.
/// Uses `OffsetDateTime` directly to avoid string roundtrip issues.
fn validate_session_with_now(
    session: &SessionState,
    queue: &QueueFile,
    timeout_hours: Option<u64>,
    now: time::OffsetDateTime,
) -> SessionValidationResult {
    // Check if task still exists and is in Doing status
    let task = match queue.tasks.iter().find(|t| t.id.trim() == session.task_id) {
        Some(t) => t,
        None => {
            return SessionValidationResult::Stale {
                reason: format!("Task {} no longer exists in queue", session.task_id),
            };
        }
    };

    if task.status != TaskStatus::Doing {
        return SessionValidationResult::Stale {
            reason: format!(
                "Task {} is not in Doing status (current: {})",
                session.task_id, task.status
            ),
        };
    }

    // Check session timeout using the injected `now`
    if let Some(timeout) = timeout_hours
        && let Ok(session_time) = timeutil::parse_rfc3339(&session.last_updated_at)
    {
        // Calculate duration by subtracting earlier from later
        if now > session_time {
            let elapsed = now - session_time;
            let hours = elapsed.whole_hours() as u64;
            if hours >= timeout {
                return SessionValidationResult::Timeout {
                    hours,
                    session: session.clone(),
                };
            }
        }
    }

    SessionValidationResult::Valid(session.clone())
}

/// Validate a session against the current queue state.
///
/// Uses the current UTC time for timeout comparisons. For deterministic testing,
/// use `validate_session_with_now` directly.
pub fn validate_session(
    session: &SessionState,
    queue: &QueueFile,
    timeout_hours: Option<u64>,
) -> SessionValidationResult {
    validate_session_with_now(
        session,
        queue,
        timeout_hours,
        time::OffsetDateTime::now_utc(),
    )
}

/// Check for existing session and return validation result.
pub fn check_session(
    cache_dir: &Path,
    queue: &QueueFile,
    timeout_hours: Option<u64>,
) -> Result<SessionValidationResult> {
    let session = match load_session(cache_dir)? {
        Some(s) => s,
        None => return Ok(SessionValidationResult::NoSession),
    };

    Ok(validate_session(&session, queue, timeout_hours))
}

/// Prompt the user for session recovery confirmation.
///
/// When `non_interactive` is true or stdin is not a TTY, returns `Ok(false)`
/// without prompting, choosing the safe default of not resuming.
pub fn prompt_session_recovery(session: &SessionState, non_interactive: bool) -> Result<bool> {
    if non_interactive || !std::io::stdin().is_terminal() {
        log::info!(
            "Non-interactive environment detected; skipping session resume for {}",
            session.task_id
        );
        return Ok(false); // Safe default: don't resume
    }

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Incomplete session detected                                 ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Task:        {}", pad_right(&session.task_id, 45));
    println!("║  Started:     {}", pad_right(&session.run_started_at, 45));
    println!(
        "║  Iterations:  {}/{}",
        session.iterations_completed, session.iterations_planned
    );
    println!(
        "║  Phase:       {}",
        pad_right(&format!("{}", session.current_phase), 45)
    );

    // Display per-phase settings if available
    if session.phase1_settings.is_some()
        || session.phase2_settings.is_some()
        || session.phase3_settings.is_some()
    {
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Phase Settings:                                             ║");

        if let Some(ref p1) = session.phase1_settings {
            let effort_str = p1
                .reasoning_effort
                .map(|e| format!(", effort={:?}", e))
                .unwrap_or_default();
            let settings_str = format!("{:?}/{}{}", p1.runner, p1.model, effort_str);
            println!("║    Phase 1:   {}", pad_right(&settings_str, 41));
        }

        if let Some(ref p2) = session.phase2_settings {
            let effort_str = p2
                .reasoning_effort
                .map(|e| format!(", effort={:?}", e))
                .unwrap_or_default();
            let settings_str = format!("{:?}/{}{}", p2.runner, p2.model, effort_str);
            println!("║    Phase 2:   {}", pad_right(&settings_str, 41));
        }

        if let Some(ref p3) = session.phase3_settings {
            let effort_str = p3
                .reasoning_effort
                .map(|e| format!(", effort={:?}", e))
                .unwrap_or_default();
            let settings_str = format!("{:?}/{}{}", p3.runner, p3.model, effort_str);
            println!("║    Phase 3:   {}", pad_right(&settings_str, 41));
        }
    }

    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    print!("Resume this session? [Y/n]: ");
    io::stdout().flush().context("flush stdout")?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).context("read stdin")?;

    let input = input.trim().to_lowercase();
    Ok(input.is_empty() || input == "y" || input == "yes")
}

/// Prompt the user for session recovery with timeout warning.
///
/// When `non_interactive` is true or stdin is not a TTY, returns `Ok(false)`
/// without prompting, choosing the safe default of not resuming.
///
/// # Arguments
/// * `session` - The session state to potentially resume
/// * `hours` - The actual age of the session in hours
/// * `threshold_hours` - The configured timeout threshold that was exceeded
/// * `non_interactive` - Whether to skip interactive prompting
pub fn prompt_session_recovery_timeout(
    session: &SessionState,
    hours: u64,
    threshold_hours: u64,
    non_interactive: bool,
) -> Result<bool> {
    if non_interactive || !std::io::stdin().is_terminal() {
        log::info!(
            "Non-interactive environment detected; skipping stale session resume for {} ({} hours old)",
            session.task_id,
            hours
        );
        return Ok(false); // Safe default: don't resume
    }

    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!(
        "║  STALE session detected ({} hours old)",
        pad_right(&hours.to_string(), 27)
    );
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Task:        {}", pad_right(&session.task_id, 45));
    println!("║  Started:     {}", pad_right(&session.run_started_at, 45));
    println!(
        "║  Last update: {}",
        pad_right(&session.last_updated_at, 45)
    );
    println!(
        "║  Iterations:  {}/{}",
        session.iterations_completed, session.iterations_planned
    );

    // Display per-phase settings if available
    if session.phase1_settings.is_some()
        || session.phase2_settings.is_some()
        || session.phase3_settings.is_some()
    {
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Phase Settings:                                             ║");

        if let Some(ref p1) = session.phase1_settings {
            let effort_str = p1
                .reasoning_effort
                .map(|e| format!(", effort={:?}", e))
                .unwrap_or_default();
            let settings_str = format!("{:?}/{}{}", p1.runner, p1.model, effort_str);
            println!("║    Phase 1:   {}", pad_right(&settings_str, 41));
        }

        if let Some(ref p2) = session.phase2_settings {
            let effort_str = p2
                .reasoning_effort
                .map(|e| format!(", effort={:?}", e))
                .unwrap_or_default();
            let settings_str = format!("{:?}/{}{}", p2.runner, p2.model, effort_str);
            println!("║    Phase 2:   {}", pad_right(&settings_str, 41));
        }

        if let Some(ref p3) = session.phase3_settings {
            let effort_str = p3
                .reasoning_effort
                .map(|e| format!(", effort={:?}", e))
                .unwrap_or_default();
            let settings_str = format!("{:?}/{}{}", p3.runner, p3.model, effort_str);
            println!("║    Phase 3:   {}", pad_right(&settings_str, 41));
        }
    }

    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!(
        "Warning: This session is older than {} hour{}.",
        threshold_hours,
        if threshold_hours == 1 { "" } else { "s" }
    );
    print!("Resume anyway? [y/N]: ");
    io::stdout().flush().context("flush stdout")?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).context("read stdin")?;

    let input = input.trim().to_lowercase();
    Ok(input == "y" || input == "yes")
}

fn pad_right(s: &str, width: usize) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - s.len()))
    }
}

/// Get the git HEAD commit hash for session tracking.
pub fn get_git_head_commit(repo_root: &Path) -> Option<String> {
    let mut command = std::process::Command::new("git");
    command
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("HEAD");

    execute_checked_command(ManagedCommand::new(
        command,
        format!("git rev-parse HEAD in {}", repo_root.display()),
        TimeoutClass::MetadataProbe,
    ))
    .ok()
    .map(|output| output.stdout_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskPriority};
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;
    use time::Duration;

    fn test_task(id: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test".to_string(),
            description: None,
            priority: TaskPriority::Medium,
            tags: vec![],
            scope: vec![],
            evidence: vec![],
            plan: vec![],
            notes: vec![],
            request: None,
            agent: None,
            created_at: None,
            updated_at: None,
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: Default::default(),
            parent_id: None,
            estimated_minutes: None,
            actual_minutes: None,
        }
    }

    /// Fixed reference timestamp for deterministic tests.
    const TEST_NOW: &str = "2026-02-07T12:00:00.000000000Z";

    fn test_now() -> time::OffsetDateTime {
        timeutil::parse_rfc3339(TEST_NOW).unwrap()
    }

    fn test_session_with_time(task_id: &str, last_updated_at: &str) -> SessionState {
        SessionState::new(
            "test-session-id".to_string(),
            task_id.to_string(),
            last_updated_at.to_string(),
            1,
            crate::contracts::Runner::Claude,
            "sonnet".to_string(),
            0,
            None,
            None, // phase_settings
        )
    }

    fn test_session(task_id: &str) -> SessionState {
        test_session_with_time(task_id, TEST_NOW)
    }

    #[test]
    fn get_git_head_commit_returns_current_head() -> Result<()> {
        let temp_dir = TempDir::new()?;
        git_test::init_repo(temp_dir.path())?;
        std::fs::write(temp_dir.path().join("README.md"), "session commit")?;
        git_test::commit_all(temp_dir.path(), "init")?;

        let commit = get_git_head_commit(temp_dir.path());
        let expected = git_test::git_output(temp_dir.path(), &["rev-parse", "HEAD"])?;

        assert_eq!(commit.as_deref(), Some(expected.as_str()));
        Ok(())
    }

    #[test]
    fn save_and_load_session_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let session = test_session("RQ-0001");

        save_session(temp_dir.path(), &session).unwrap();
        let loaded = load_session(temp_dir.path()).unwrap().unwrap();

        assert_eq!(loaded.session_id, session.session_id);
        assert_eq!(loaded.task_id, session.task_id);
        assert_eq!(loaded.iterations_planned, session.iterations_planned);
    }

    #[test]
    fn clear_session_removes_file() {
        let temp_dir = TempDir::new().unwrap();
        let session = test_session("RQ-0001");

        save_session(temp_dir.path(), &session).unwrap();
        assert!(session_exists(temp_dir.path()));

        clear_session(temp_dir.path()).unwrap();
        assert!(!session_exists(temp_dir.path()));
    }

    #[test]
    fn validate_session_valid_when_task_doing() {
        let session = test_session("RQ-0001");
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        let result = validate_session(&session, &queue, None);
        assert!(matches!(result, SessionValidationResult::Valid(_)));
    }

    #[test]
    fn validate_session_stale_when_task_not_doing() {
        let session = test_session("RQ-0001");
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Todo)],
        };

        let result = validate_session(&session, &queue, None);
        assert!(matches!(result, SessionValidationResult::Stale { .. }));
    }

    #[test]
    fn validate_session_stale_when_task_missing() {
        let session = test_session("RQ-0001");
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0002", TaskStatus::Doing)],
        };

        let result = validate_session(&session, &queue, None);
        assert!(matches!(result, SessionValidationResult::Stale { .. }));
    }

    #[test]
    fn check_session_returns_no_session_when_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        let queue = QueueFile {
            version: 1,
            tasks: vec![],
        };

        let result = check_session(temp_dir.path(), &queue, None).unwrap();
        assert_eq!(result, SessionValidationResult::NoSession);
    }

    #[test]
    fn session_path_returns_correct_path() {
        let temp_dir = TempDir::new().unwrap();
        let path = session_path(temp_dir.path());
        assert_eq!(path, temp_dir.path().join("session.jsonc"));
    }

    #[test]
    fn prompt_session_recovery_returns_false_when_non_interactive() {
        let session = test_session("RQ-0001");
        // When non_interactive=true, should return false without prompting
        let result = prompt_session_recovery(&session, true).unwrap();
        assert!(
            !result,
            "non_interactive=true should return false (do not resume)"
        );
    }

    #[test]
    fn prompt_session_recovery_timeout_returns_false_when_non_interactive() {
        let session = test_session("RQ-0001");
        // When non_interactive=true, should return false without prompting
        let result = prompt_session_recovery_timeout(&session, 48, 24, true).unwrap();
        assert!(
            !result,
            "non_interactive=true should return false (do not resume)"
        );
    }

    #[test]
    fn validate_session_returns_timeout_when_older_than_threshold() {
        // Use deterministic "now" and session time 48 hours before that
        let now = test_now();
        let session_time = now - Duration::hours(48);
        let session =
            test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        // With 24-hour threshold, should timeout
        let result = validate_session_with_now(&session, &queue, Some(24), now);
        match result {
            SessionValidationResult::Timeout {
                hours,
                session: timed_out,
            } => {
                assert_eq!(hours, 48, "Expected exactly 48 hours, got {hours}");
                assert_eq!(timed_out.task_id, session.task_id);
                assert_eq!(timed_out.session_id, session.session_id);
            }
            other => panic!("expected Timeout, got {other:?}"),
        }
    }

    /// Regression test for RQ-0632: check_session must return Timeout with the embedded
    /// session state so callers don't need to re-load (which could panic if session.json
    /// disappears between the first load and the re-load).
    ///
    /// Note: This test uses the real wall-clock time via `check_session`, so we only assert
    /// that the result is a Timeout with the session embedded, not the exact hours value.
    #[test]
    fn check_session_returns_timeout_and_includes_loaded_session() {
        let temp_dir = TempDir::new().unwrap();

        // Create a session with a very old timestamp (1 year ago) to ensure it times out
        // regardless of when the test runs
        let session_time = time::OffsetDateTime::now_utc() - Duration::days(365);
        let session =
            test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());

        save_session(temp_dir.path(), &session).unwrap();

        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        let result = check_session(temp_dir.path(), &queue, Some(24)).unwrap();

        match result {
            SessionValidationResult::Timeout {
                hours,
                session: timed_out,
            } => {
                // Just verify we got a reasonable timeout value (at least 24 hours)
                assert!(hours >= 24, "Expected at least 24 hours, got {hours}");
                assert_eq!(timed_out.task_id, session.task_id);
                assert_eq!(timed_out.session_id, session.session_id);
                assert_eq!(timed_out.last_updated_at, session.last_updated_at);
            }
            other => panic!("expected Timeout, got {other:?}"),
        }
    }

    #[test]
    fn validate_session_returns_valid_when_within_custom_threshold() {
        // Session 12 hours old with 48-hour threshold should be valid
        let now = test_now();
        let session_time = now - Duration::hours(12);
        let session =
            test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        // With 48-hour threshold, 12-hour session should be valid
        let result = validate_session_with_now(&session, &queue, Some(48), now);
        assert!(
            matches!(result, SessionValidationResult::Valid(_)),
            "Session within custom threshold should return Valid"
        );
    }

    #[test]
    fn validate_session_returns_valid_when_within_default_threshold() {
        // Session 1 hour old with 24-hour threshold should be valid
        let now = test_now();
        let session_time = now - Duration::hours(1);
        let session =
            test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        // With default 24-hour threshold, 1-hour session should be valid
        let result = validate_session_with_now(&session, &queue, Some(24), now);
        assert!(
            matches!(result, SessionValidationResult::Valid(_)),
            "Session within default threshold should return Valid"
        );
    }

    #[test]
    fn validate_session_returns_valid_when_no_timeout_configured() {
        let session = test_session("RQ-0001");
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        // With no timeout configured (None), session should always be valid
        let result = validate_session(&session, &queue, None);
        assert!(
            matches!(result, SessionValidationResult::Valid(_)),
            "Session should be Valid when no timeout is configured"
        );
    }

    #[test]
    fn validate_session_invalid_last_updated_does_not_timeout() {
        // Session with unparsable timestamp should not trigger timeout (kept for safety)
        let session = test_session_with_time("RQ-0001", "not-a-valid-timestamp");
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        // Even with a short timeout, invalid timestamp means we can't compute age
        let result = validate_session_with_now(
            &session,
            &queue,
            Some(1), // 1 hour threshold
            test_now(),
        );
        assert!(
            matches!(result, SessionValidationResult::Valid(_)),
            "Session with invalid timestamp should be Valid (can't compute timeout)"
        );
    }

    #[test]
    fn validate_session_exact_boundary_returns_timeout() {
        // Session exactly at the threshold boundary should timeout (>=)
        let now = test_now();
        let session_time = now - Duration::hours(24); // exactly 24 hours old
        let session =
            test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        // With 24-hour threshold, exactly 24 hours should timeout
        let result = validate_session_with_now(&session, &queue, Some(24), now);
        assert!(
            matches!(result, SessionValidationResult::Timeout { .. }),
            "Session exactly at threshold should timeout"
        );
    }

    #[test]
    fn validate_session_future_timestamp_no_timeout() {
        // Session with future timestamp should not timeout (now <= session_time)
        let now = test_now();
        let session_time = now + Duration::hours(1); // 1 hour in the future
        let session =
            test_session_with_time("RQ-0001", &timeutil::format_rfc3339(session_time).unwrap());
        let queue = QueueFile {
            version: 1,
            tasks: vec![test_task("RQ-0001", TaskStatus::Doing)],
        };

        let result = validate_session_with_now(&session, &queue, Some(1), now);
        assert!(
            matches!(result, SessionValidationResult::Valid(_)),
            "Session with future timestamp should be Valid (no timeout)"
        );
    }

    #[test]
    fn increment_session_progress_updates_and_persists() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // Create initial session
        let session = test_session("RQ-0001");
        save_session(&cache_dir, &session).unwrap();

        assert_eq!(session.tasks_completed_in_loop, 0);

        // Increment once
        increment_session_progress(&cache_dir).unwrap();
        let loaded = load_session(&cache_dir).unwrap().unwrap();
        assert_eq!(loaded.tasks_completed_in_loop, 1);

        // Increment again
        increment_session_progress(&cache_dir).unwrap();
        let loaded = load_session(&cache_dir).unwrap().unwrap();
        assert_eq!(loaded.tasks_completed_in_loop, 2);
    }

    #[test]
    fn increment_session_progress_handles_missing_session() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        // No session exists - should succeed without error
        increment_session_progress(&cache_dir).unwrap();
    }
}
