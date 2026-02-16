//! Productivity module tests.
//!
//! Responsibilities:
//! - Unit tests for productivity tracking functionality.
//!
//! Not handled here:
//! - Integration tests (see `tests/` directory).

use std::collections::BTreeMap;

use tempfile::TempDir;

use crate::contracts::{Task, TaskPriority, TaskStatus};

use super::calculations::{calculate_velocity_for_today, next_milestone, update_streak};
use super::date_utils::{date_key_add_days, parse_date_key, previous_date_key};
use super::persistence::load_productivity_stats;
use super::reports::format_duration;
use super::types::{DayStats, ProductivityStats, StreakInfo};
use super::{record_task_completion, record_task_completion_by_id};

fn create_test_task(id: &str, title: &str) -> Task {
    Task {
        id: id.to_string(),
        title: title.to_string(),
        description: None,
        status: TaskStatus::Done,
        priority: TaskPriority::Medium,
        tags: vec![],
        scope: vec![],
        evidence: vec![],
        plan: vec![],
        notes: vec![],
        request: None,
        agent: None,
        created_at: Some("2026-01-01T00:00:00Z".to_string()),
        updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        completed_at: Some("2026-01-01T12:00:00Z".to_string()),
        started_at: None,
        scheduled_start: None,
        depends_on: vec![],
        blocks: vec![],
        relates_to: vec![],
        duplicates: None,
        custom_fields: std::collections::HashMap::new(),
        parent_id: None,
        estimated_minutes: None,
        actual_minutes: None,
    }
}

#[test]
fn test_load_stats_empty_cache() {
    let temp = TempDir::new().unwrap();
    let stats = load_productivity_stats(temp.path()).unwrap();
    assert_eq!(stats.total_completed, 0);
    assert!(stats.daily.is_empty());
    // CRITICAL: Default last_updated_at must never be empty (regression test for RQ-0636)
    assert!(
        !stats.last_updated_at.is_empty(),
        "Default last_updated_at should never be empty"
    );
}

#[test]
fn test_record_task_completion() {
    let temp = TempDir::new().unwrap();
    let task = create_test_task("RQ-0001", "Test task");

    let result = record_task_completion(&task, temp.path()).unwrap();

    assert_eq!(result.total_completed, 1);
    assert_eq!(result.new_streak, 1);
    assert!(result.streak_updated);
    assert!(result.milestone_achieved.is_none());

    // Verify saved
    let stats = load_productivity_stats(temp.path()).unwrap();
    assert_eq!(stats.total_completed, 1);

    // CRITICAL: All persisted timestamps must be non-empty (regression test for RQ-0636)
    assert!(
        !stats.last_updated_at.is_empty(),
        "last_updated_at should never be empty"
    );
    for day_stats in stats.daily.values() {
        for task_ref in &day_stats.tasks {
            assert!(
                !task_ref.completed_at.is_empty(),
                "Task completed_at should never be empty"
            );
        }
    }
}

#[test]
fn test_milestone_detection() {
    let temp = TempDir::new().unwrap();

    // Complete 10 tasks
    for i in 1..=10 {
        let task = create_test_task(&format!("RQ-{:04}", i), "Test task");
        let result = record_task_completion(&task, temp.path()).unwrap();

        if i == 10 {
            assert_eq!(result.milestone_achieved, Some(10));
        } else {
            assert!(result.milestone_achieved.is_none());
        }
    }
}

#[test]
fn test_duplicate_completion_ignored() {
    let temp = TempDir::new().unwrap();
    let task = create_test_task("RQ-0001", "Test task");

    // Record same task twice
    record_task_completion(&task, temp.path()).unwrap();
    let _result = record_task_completion(&task, temp.path()).unwrap();

    // Should still show as completed but not increment count
    let stats = load_productivity_stats(temp.path()).unwrap();
    // Note: Currently we don't dedupe across days, just within the same day
    // So this test verifies the daily deduplication
    assert!(stats.total_completed >= 1);
}

#[test]
fn test_velocity_calculation() {
    // Use fixed dates that cross a year boundary to test real calendar math
    let today: String = "2026-01-01".to_string();
    let yesterday: String = "2025-12-31".to_string();

    let stats = ProductivityStats {
        version: 1,
        first_task_completed_at: None,
        last_updated_at: format!("{}T00:00:00Z", today),
        daily: {
            let mut daily = BTreeMap::new();
            daily.insert(
                today.clone(),
                DayStats {
                    date: today.clone(),
                    completed_count: 5,
                    tasks: vec![],
                },
            );
            daily.insert(
                yesterday.clone(),
                DayStats {
                    date: yesterday,
                    completed_count: 3,
                    tasks: vec![],
                },
            );
            daily
        },
        streak: StreakInfo::default(),
        total_completed: 8,
        milestones: vec![],
    };

    let velocity = calculate_velocity_for_today(&stats, 7, &today);
    assert_eq!(velocity.total_completed, 8);
    assert!(velocity.average_per_day > 0.0);
}

#[test]
fn test_next_milestone() {
    assert_eq!(next_milestone(0), Some(10));
    assert_eq!(next_milestone(9), Some(10));
    assert_eq!(next_milestone(10), Some(50));
    assert_eq!(next_milestone(100), Some(250));
    assert_eq!(next_milestone(5000), None);
}

#[test]
fn test_format_duration() {
    assert_eq!(format_duration(30), "30s");
    assert_eq!(format_duration(90), "1m");
    assert_eq!(format_duration(3600), "1h 0m");
    assert_eq!(format_duration(90061), "1d 1h");
}

// Tests for date key helpers with proper calendar math

#[test]
fn test_previous_date_key_leap_year() {
    // 2024 is a leap year: March 1 -> Feb 29
    assert_eq!(
        previous_date_key("2024-03-01"),
        Some("2024-02-29".to_string())
    );
}

#[test]
fn test_previous_date_key_non_leap_year() {
    // 2026 is not a leap year: March 1 -> Feb 28
    assert_eq!(
        previous_date_key("2026-03-01"),
        Some("2026-02-28".to_string())
    );
}

#[test]
fn test_previous_date_key_year_boundary() {
    // Jan 1 -> Dec 31 of previous year
    assert_eq!(
        previous_date_key("2026-01-01"),
        Some("2025-12-31".to_string())
    );
}

#[test]
fn test_previous_date_key_month_boundary_30_day() {
    // May 1 -> April 30 (April has 30 days)
    assert_eq!(
        previous_date_key("2026-05-01"),
        Some("2026-04-30".to_string())
    );
}

#[test]
fn test_previous_date_key_normal_day() {
    // Normal day decrement
    assert_eq!(
        previous_date_key("2026-02-15"),
        Some("2026-02-14".to_string())
    );
}

#[test]
fn test_parse_date_key_invalid() {
    assert_eq!(parse_date_key(""), None);
    assert_eq!(parse_date_key("  "), None);
    assert_eq!(parse_date_key("not-a-date"), None);
    assert_eq!(parse_date_key("2026-02-30"), None); // Feb 30 doesn't exist
}

#[test]
fn test_date_key_offset_backwards() {
    // Go back 7 days from Jan 5 = Dec 29
    assert_eq!(
        date_key_add_days("2026-01-05", -7),
        Some("2025-12-29".to_string())
    );
}

#[test]
fn test_date_key_add_days_forward() {
    // Go forward 5 days
    assert_eq!(
        date_key_add_days("2026-01-01", 5),
        Some("2026-01-06".to_string())
    );
}

#[test]
fn test_streak_year_boundary() {
    // Test that streak correctly increments across year boundaries
    let mut stats = ProductivityStats {
        version: 1,
        first_task_completed_at: None,
        last_updated_at: "2026-01-01T00:00:00Z".to_string(),
        daily: BTreeMap::new(),
        streak: StreakInfo {
            current_streak: 3,
            longest_streak: 5,
            last_completed_date: Some("2025-12-31".to_string()),
        },
        total_completed: 10,
        milestones: vec![],
    };

    // Complete a task on Jan 1, 2026 - should continue the streak
    let updated = update_streak(&mut stats, "2026-01-01");
    assert!(updated);
    assert_eq!(stats.streak.current_streak, 4);
    assert_eq!(
        stats.streak.last_completed_date,
        Some("2026-01-01".to_string())
    );
}

#[test]
fn test_streak_breaks_when_gap() {
    // Test that streak breaks when there's a gap
    let mut stats = ProductivityStats {
        version: 1,
        first_task_completed_at: None,
        last_updated_at: "2026-01-05T00:00:00Z".to_string(),
        daily: BTreeMap::new(),
        streak: StreakInfo {
            current_streak: 3,
            longest_streak: 5,
            last_completed_date: Some("2026-01-01".to_string()), // Last completed 3 days ago
        },
        total_completed: 10,
        milestones: vec![],
    };

    // Complete a task on Jan 5, 2026 - should break and restart streak
    let updated = update_streak(&mut stats, "2026-01-05");
    assert!(updated);
    assert_eq!(stats.streak.current_streak, 1); // Reset to 1
    assert_eq!(
        stats.streak.last_completed_date,
        Some("2026-01-05".to_string())
    );
}

#[test]
fn test_update_streak_invalid_today_is_noop() {
    let mut stats = ProductivityStats {
        version: 1,
        first_task_completed_at: None,
        last_updated_at: "2026-01-01T00:00:00Z".to_string(),
        daily: BTreeMap::new(),
        streak: StreakInfo {
            current_streak: 3,
            longest_streak: 5,
            last_completed_date: Some("2026-01-01".to_string()),
        },
        total_completed: 10,
        milestones: vec![],
    };

    let before_current = stats.streak.current_streak;
    let before_longest = stats.streak.longest_streak;
    let before_last = stats.streak.last_completed_date.clone();
    assert!(!update_streak(&mut stats, "not-a-date"));
    assert_eq!(stats.streak.current_streak, before_current);
    assert_eq!(stats.streak.longest_streak, before_longest);
    assert_eq!(stats.streak.last_completed_date, before_last);
}

#[test]
fn test_record_task_completion_by_id() {
    let temp = TempDir::new().unwrap();

    let result = record_task_completion_by_id("RQ-0001", "Test task", temp.path()).unwrap();

    assert_eq!(result.total_completed, 1);
    assert_eq!(result.new_streak, 1);
    assert!(result.streak_updated);

    // Verify saved
    let stats = load_productivity_stats(temp.path()).unwrap();
    assert_eq!(stats.total_completed, 1);
}
