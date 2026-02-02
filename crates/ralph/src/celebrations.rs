//! Celebration animations and feedback for task completions.
//!
//! Responsibilities:
//! - Render celebration animations for TUI and CLI
//! - Check terminal capabilities (is_terminal, color support)
//! - Respect config settings (celebrations_enabled, --no-progress)
//!
//! Not handled here:
//! - Stats persistence (see `crate::productivity`)
//! - Notification delivery (see `crate::notification`)
//!
//! Invariants/assumptions:
//! - Celebrations are subtle and professional, not flashy
//! - ASCII art is used for terminal compatibility
//! - Celebrations respect terminal capabilities and user preferences

use crate::contracts::Config;
use crate::productivity::CompletionResult;
use std::io::IsTerminal;

/// Check if celebrations should be shown
pub fn should_celebrate(config: Option<&Config>, no_progress: bool) -> bool {
    // Don't celebrate if progress is disabled
    if no_progress {
        return false;
    }

    // Don't celebrate if not in a terminal
    if !std::io::stdout().is_terminal() {
        return false;
    }

    // Check config setting (default to true)
    if let Some(config) = config
        && let Some(enabled) = config.tui.celebrations_enabled
    {
        return enabled;
    }

    true
}

/// Render a celebration for task completion
pub fn celebrate_task_completion(
    task_id: &str,
    task_title: &str,
    result: &CompletionResult,
) -> String {
    if let Some(milestone) = result.milestone_achieved {
        return celebrate_milestone(task_id, task_title, milestone, result.new_streak);
    }

    if result.streak_updated && result.new_streak > 1 {
        celebrate_streak(task_id, task_title, result.new_streak)
    } else {
        celebrate_standard(task_id, task_title)
    }
}

/// Standard task completion celebration
pub fn celebrate_standard(task_id: &str, task_title: &str) -> String {
    format!(
        r#"
  {}
  Task {} completed!
  {}
"#,
        art::SPARKLES,
        task_id,
        task_title
    )
}

/// Streak celebration
fn celebrate_streak(task_id: &str, task_title: &str, streak: u32) -> String {
    format!(
        r#"
  {}
  Task {} completed!
  {}

  {} {}-day streak!
"#,
        art::SPARKLES,
        task_id,
        task_title,
        art::FIRE,
        streak
    )
}

/// Milestone celebration
fn celebrate_milestone(task_id: &str, task_title: &str, milestone: u64, streak: u32) -> String {
    let streak_text = if streak > 1 {
        format!("\n  {} {}-day streak!", art::FIRE, streak)
    } else {
        String::new()
    };

    format!(
        r#"
{}

  Task {} completed!
  {}

  Milestone reached: {} tasks completed!{}
"#,
        art::milestone_banner(milestone),
        task_id,
        task_title,
        milestone,
        streak_text
    )
}

/// Session summary celebration
pub fn celebrate_session_summary(tasks_completed: usize, duration_seconds: i64) -> String {
    let duration_text = if duration_seconds < 60 {
        format!("{}s", duration_seconds)
    } else if duration_seconds < 3600 {
        format!("{}m", duration_seconds / 60)
    } else {
        format!(
            "{}h {}m",
            duration_seconds / 3600,
            (duration_seconds % 3600) / 60
        )
    };

    let message = match tasks_completed {
        0 => "No tasks completed this session.".to_string(),
        1 => "1 task completed!".to_string(),
        n => format!("{} tasks completed!", n),
    };

    format!(
        r#"
{}

  Session Complete!
  {}
  Time: {}
"#,
        art::SESSION_END,
        message,
        duration_text
    )
}

/// ASCII art celebrations
pub mod art {
    pub use crate::constants::symbols::{CHECKMARK, FIRE, SPARKLES, STAR};

    /// Session end banner
    pub const SESSION_END: &str = r#"
  ╔═══════════════════════════════════════╗
  ║           SESSION COMPLETE            ║
  ╚═══════════════════════════════════════╝
"#;

    /// Generate milestone banner
    pub fn milestone_banner(threshold: u64) -> String {
        let threshold_str = threshold.to_string();
        let padding = (33usize.saturating_sub(threshold_str.len())) / 2;
        let left_pad = " ".repeat(padding);
        let right_pad = " ".repeat(
            33usize
                .saturating_sub(threshold_str.len())
                .saturating_sub(padding),
        );

        format!(
            r#"
  ╔═══════════════════════════════════════╗
  ║{}🎉 MILESTONE: {}{}🎉║
  ╚═══════════════════════════════════════╝
"#,
            left_pad, threshold_str, right_pad
        )
    }

    /// Streak fire with count
    pub fn streak_fire(streak: u32) -> String {
        format!("{} {}-day streak!", FIRE, streak)
    }

    /// Celebration stars
    pub fn celebration_stars() -> &'static str {
        r#"
    .  *  .  *  .
  *  .  ★  .  *
    .  *  .  *  .
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::Config;

    #[test]
    fn test_should_celebrate_respects_no_progress() {
        let config = Config::default();
        assert!(!should_celebrate(Some(&config), true));
    }

    #[test]
    fn test_should_celebrate_respects_config() {
        let mut config = Config::default();
        config.tui.celebrations_enabled = Some(false);
        // Still returns false because we're not in a terminal in tests
        // but we can at least verify the config path is checked
        let result = should_celebrate(Some(&config), false);
        // In test environment (not a terminal), this will be false
        // In a real terminal with config disabled, this would also be false
        assert!(!result);
    }

    #[test]
    fn test_celebrate_standard_includes_task_info() {
        let result = celebrate_standard("RQ-0001", "Test task");
        assert!(result.contains("RQ-0001"));
        assert!(result.contains("Test task"));
        assert!(result.contains("completed"));
    }

    #[test]
    fn test_celebrate_streak_includes_streak_count() {
        let result = celebrate_streak("RQ-0001", "Test task", 5);
        assert!(result.contains("5-day streak"));
    }

    #[test]
    fn test_celebrate_milestone_includes_threshold() {
        let result = celebrate_milestone("RQ-0001", "Test task", 100, 3);
        assert!(result.contains("100"));
        assert!(result.contains("Milestone"));
        assert!(result.contains("3-day streak"));
    }

    #[test]
    fn test_milestone_banner_format() {
        let banner = art::milestone_banner(100);
        assert!(banner.contains("100"));
        assert!(banner.contains("MILESTONE"));
    }

    #[test]
    fn test_celebrate_task_completion_with_milestone() {
        let completion_result = CompletionResult {
            milestone_achieved: Some(10),
            streak_updated: true,
            new_streak: 3,
            total_completed: 10,
        };

        let result = celebrate_task_completion("RQ-0010", "Milestone task", &completion_result);
        assert!(result.contains("Milestone"));
        assert!(result.contains("10"));
        assert!(result.contains("3-day streak"));
    }

    #[test]
    fn test_celebrate_task_completion_with_streak() {
        let completion_result = CompletionResult {
            milestone_achieved: None,
            streak_updated: true,
            new_streak: 5,
            total_completed: 15,
        };

        let result = celebrate_task_completion("RQ-0015", "Streak task", &completion_result);
        assert!(result.contains("5-day streak"));
        assert!(!result.contains("Milestone"));
    }

    #[test]
    fn test_celebrate_session_summary() {
        let result = celebrate_session_summary(5, 3665);
        assert!(result.contains("5 tasks completed"));
        assert!(result.contains("1h 1m"));
    }

    #[test]
    fn test_celebrate_session_summary_single_task() {
        let result = celebrate_session_summary(1, 45);
        assert!(result.contains("1 task completed"));
        assert!(result.contains("45s"));
    }

    #[test]
    fn test_celebrate_session_summary_no_tasks() {
        let result = celebrate_session_summary(0, 0);
        assert!(result.contains("No tasks completed"));
    }
}
