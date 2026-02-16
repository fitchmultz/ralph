//! Productivity calculations and core business logic.
//!
//! Responsibilities:
//! - Record task completions and update stats.
//! - Calculate streaks, velocity, and estimation accuracy.
//! - Check and record milestones.
//!
//! Not handled here:
//! - Persistence (see `super::persistence`).
//! - Data structure definitions (see `super::types`).
//! - Report formatting (see `super::reports`).

use anyhow::Result;
use time::OffsetDateTime;

use crate::constants::milestones::MILESTONE_THRESHOLDS;
use crate::contracts::Task;
use crate::timeutil;

use super::date_utils::{date_key_add_days, format_date_key, parse_date_key, previous_date_key};
use super::types::{
    CompletionResult, DayStats, EstimationMetrics, ProductivityStats, TaskEstimationPoint,
    VelocityMetrics,
};

/// Record a task completion and update stats
pub fn record_task_completion(
    task: &Task,
    cache_dir: &std::path::Path,
) -> Result<CompletionResult> {
    let mut stats = super::persistence::load_productivity_stats(cache_dir)?;
    let result = update_stats_with_completion(&mut stats, task)?;
    super::persistence::save_productivity_stats(&stats, cache_dir)?;
    Ok(result)
}

/// Record a task completion by ID and title (for cases where Task isn't available)
pub fn record_task_completion_by_id(
    task_id: &str,
    task_title: &str,
    cache_dir: &std::path::Path,
) -> Result<CompletionResult> {
    let mut stats = super::persistence::load_productivity_stats(cache_dir)?;
    let result = update_stats_with_completion_ref(
        &mut stats,
        task_id,
        task_title,
        &timeutil::now_utc_rfc3339()?,
    )?;
    super::persistence::save_productivity_stats(&stats, cache_dir)?;
    Ok(result)
}

/// Update stats with a task completion (internal)
fn update_stats_with_completion(
    stats: &mut ProductivityStats,
    task: &Task,
) -> Result<CompletionResult> {
    let completed_at = task
        .completed_at
        .clone()
        .unwrap_or_else(timeutil::now_utc_rfc3339_or_fallback);

    update_stats_with_completion_ref(stats, &task.id, &task.title, &completed_at)
}

/// Update stats with a task completion reference (internal)
fn update_stats_with_completion_ref(
    stats: &mut ProductivityStats,
    task_id: &str,
    task_title: &str,
    completed_at: &str,
) -> Result<CompletionResult> {
    let now = timeutil::now_utc_rfc3339()?;
    let today = now.split('T').next().unwrap_or(&now).to_string();

    // Update first completion timestamp
    if stats.first_task_completed_at.is_none() {
        stats.first_task_completed_at = Some(completed_at.to_string());
    }

    // Update daily stats
    let day_stats = stats
        .daily
        .entry(today.clone())
        .or_insert_with(|| DayStats {
            date: today.clone(),
            completed_count: 0,
            tasks: Vec::new(),
        });

    // Check if task already recorded today (avoid duplicates)
    if !day_stats.tasks.iter().any(|t| t.id == task_id) {
        day_stats.completed_count += 1;
        day_stats.tasks.push(super::types::CompletedTaskRef {
            id: task_id.to_string(),
            title: task_title.to_string(),
            completed_at: completed_at.to_string(),
        });
    }

    // Update total completed
    stats.total_completed += 1;

    // Update streak
    let streak_updated = update_streak(stats, &today);

    // Check for milestone
    let milestone_achieved = check_milestone(stats);

    stats.last_updated_at = now;

    Ok(CompletionResult {
        milestone_achieved,
        streak_updated,
        new_streak: stats.streak.current_streak,
        total_completed: stats.total_completed,
    })
}

/// Update streak based on completion date
pub fn update_streak(stats: &mut ProductivityStats, today: &str) -> bool {
    // Defensive: avoid poisoning persisted stats with an invalid key.
    if parse_date_key(today).is_none() {
        return false;
    }

    let yesterday = previous_date_key(today);

    match &stats.streak.last_completed_date {
        Some(last_date) if last_date.as_str() == today => {
            // Already completed today, streak unchanged
            false
        }
        Some(last_date) if yesterday.as_deref() == Some(last_date.as_str()) => {
            // Completed yesterday, increment streak
            stats.streak.current_streak += 1;
            stats.streak.last_completed_date = Some(today.to_string());
            if stats.streak.current_streak > stats.streak.longest_streak {
                stats.streak.longest_streak = stats.streak.current_streak;
            }
            true
        }
        _ => {
            // Streak broken or first completion, start new streak
            stats.streak.current_streak = 1;
            stats.streak.last_completed_date = Some(today.to_string());
            if stats.streak.current_streak > stats.streak.longest_streak {
                stats.streak.longest_streak = stats.streak.current_streak;
            }
            true
        }
    }
}

/// Check if a milestone was achieved and record it
fn check_milestone(stats: &mut ProductivityStats) -> Option<u64> {
    for &threshold in MILESTONE_THRESHOLDS {
        if stats.total_completed == threshold {
            // Check if already recorded
            if !stats.milestones.iter().any(|m| m.threshold == threshold) {
                let now = timeutil::now_utc_rfc3339_or_fallback();
                stats.milestones.push(super::types::Milestone {
                    threshold,
                    achieved_at: now,
                    celebrated: false,
                });
                return Some(threshold);
            }
        }
    }
    None
}

/// Mark a milestone as celebrated
pub fn mark_milestone_celebrated(cache_dir: &std::path::Path, threshold: u64) -> Result<()> {
    let mut stats = super::persistence::load_productivity_stats(cache_dir)?;

    if let Some(milestone) = stats
        .milestones
        .iter_mut()
        .find(|m| m.threshold == threshold)
    {
        milestone.celebrated = true;
        super::persistence::save_productivity_stats(&stats, cache_dir)?;
    }

    Ok(())
}

/// Calculate velocity metrics for the given number of days.
pub fn calculate_velocity(stats: &ProductivityStats, days: u32) -> VelocityMetrics {
    let today = format_date_key(OffsetDateTime::now_utc().date());
    calculate_velocity_for_today(stats, days, &today)
}

/// Calculate estimation accuracy metrics from completed tasks.
/// Only includes tasks that have both estimated_minutes and actual_minutes set.
pub fn calculate_estimation_metrics(tasks: &[Task]) -> EstimationMetrics {
    let estimation_points: Vec<TaskEstimationPoint> = tasks
        .iter()
        .filter_map(|task| {
            let estimated = task.estimated_minutes?;
            let actual = task.actual_minutes?;
            if estimated == 0 {
                return None;
            }
            let ratio = actual as f64 / estimated as f64;
            Some(TaskEstimationPoint {
                task_id: task.id.clone(),
                task_title: task.title.clone(),
                estimated_minutes: estimated,
                actual_minutes: actual,
                accuracy_ratio: ratio,
            })
        })
        .collect();

    let count = estimation_points.len();
    if count == 0 {
        return EstimationMetrics {
            tasks_analyzed: 0,
            average_accuracy_ratio: 0.0,
            median_accuracy_ratio: 0.0,
            within_25_percent: 0.0,
            average_absolute_error_minutes: 0.0,
        };
    }

    let ratios: Vec<f64> = estimation_points.iter().map(|p| p.accuracy_ratio).collect();
    let average_ratio = ratios.iter().sum::<f64>() / count as f64;

    let mut sorted_ratios = ratios.clone();
    sorted_ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_ratio = if count % 2 == 1 {
        sorted_ratios[count / 2]
    } else {
        (sorted_ratios[count / 2 - 1] + sorted_ratios[count / 2]) / 2.0
    };

    let within_25 = estimation_points
        .iter()
        .filter(|p| p.accuracy_ratio >= 0.75 && p.accuracy_ratio <= 1.25)
        .count() as f64
        / count as f64
        * 100.0;

    let avg_abs_error = estimation_points
        .iter()
        .map(|p| (p.actual_minutes as f64 - p.estimated_minutes as f64).abs())
        .sum::<f64>()
        / count as f64;

    EstimationMetrics {
        tasks_analyzed: count as u32,
        average_accuracy_ratio: average_ratio,
        median_accuracy_ratio: median_ratio,
        within_25_percent: within_25,
        average_absolute_error_minutes: avg_abs_error,
    }
}

pub fn calculate_velocity_for_today(
    stats: &ProductivityStats,
    days: u32,
    today: &str,
) -> VelocityMetrics {
    let days = days.max(1);

    // Defensive: if callers pass an invalid key, treat it as "no data".
    if parse_date_key(today).is_none() {
        return VelocityMetrics {
            days,
            total_completed: 0,
            average_per_day: 0.0,
            best_day: None,
        };
    }

    let mut total = 0u32;
    let mut best_day: Option<(String, u32)> = None;

    for i in 0..days {
        let Some(date) = date_key_add_days(today, -(i as i64)) else {
            continue;
        };
        if let Some(day_stats) = stats.daily.get(&date) {
            total += day_stats.completed_count;
            if best_day.is_none() || day_stats.completed_count > best_day.as_ref().unwrap().1 {
                best_day = Some((date, day_stats.completed_count));
            }
        }
    }

    let average_per_day = total as f64 / days as f64;

    VelocityMetrics {
        days,
        total_completed: total,
        average_per_day,
        best_day,
    }
}

/// Get the next milestone threshold
pub fn next_milestone(current_total: u64) -> Option<u64> {
    MILESTONE_THRESHOLDS
        .iter()
        .copied()
        .find(|&t| t > current_total)
}

/// Get recent completed tasks
pub fn recent_completed_tasks(
    stats: &ProductivityStats,
    limit: usize,
) -> Vec<super::types::CompletedTaskRef> {
    let mut out = Vec::new();
    for (_day, day_stats) in stats.daily.iter().rev() {
        for task in day_stats.tasks.iter().rev() {
            out.push(task.clone());
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}
