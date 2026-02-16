//! Productivity report builders and display functions.
//!
//! Responsibilities:
//! - Build structured reports from stats data.
//! - Print formatted reports to stdout.
//!
//! Not handled here:
//! - Calculations and business logic (see `super::calculations`).
//! - Data persistence (see `super::persistence`).

use crate::contracts::Task;

use super::calculations::{
    calculate_estimation_metrics, calculate_velocity, next_milestone, recent_completed_tasks,
};
use super::types::{
    ProductivityEstimationReport, ProductivityStats, ProductivityStreakReport,
    ProductivitySummaryReport, ProductivityVelocityReport,
};

/// Build a summary report
pub fn build_summary_report(stats: &ProductivityStats, recent: usize) -> ProductivitySummaryReport {
    let recent_completions = recent_completed_tasks(stats, recent);
    ProductivitySummaryReport {
        total_completed: stats.total_completed,
        current_streak: stats.streak.current_streak,
        longest_streak: stats.streak.longest_streak,
        last_completed_date: stats.streak.last_completed_date.clone(),
        next_milestone: next_milestone(stats.total_completed),
        milestones: stats.milestones.clone(),
        recent_completions,
    }
}

/// Build a streak report
pub fn build_streak_report(stats: &ProductivityStats) -> ProductivityStreakReport {
    ProductivityStreakReport {
        current_streak: stats.streak.current_streak,
        longest_streak: stats.streak.longest_streak,
        last_completed_date: stats.streak.last_completed_date.clone(),
    }
}

/// Build a velocity report
pub fn build_velocity_report(stats: &ProductivityStats, days: u32) -> ProductivityVelocityReport {
    let metrics = calculate_velocity(stats, days);
    ProductivityVelocityReport {
        window_days: days.max(1),
        total_completed: metrics.total_completed,
        average_per_day: metrics.average_per_day,
        best_day: metrics.best_day,
    }
}

/// Build an estimation report from tasks
pub fn build_estimation_report(tasks: &[Task]) -> ProductivityEstimationReport {
    let metrics = calculate_estimation_metrics(tasks);
    ProductivityEstimationReport {
        tasks_analyzed: metrics.tasks_analyzed,
        average_accuracy_ratio: metrics.average_accuracy_ratio,
        median_accuracy_ratio: metrics.median_accuracy_ratio,
        within_25_percent: metrics.within_25_percent,
        average_absolute_error_minutes: metrics.average_absolute_error_minutes,
    }
}

/// Print summary report in text format
pub fn print_summary_report_text(report: &ProductivitySummaryReport) {
    println!("Productivity Summary");
    println!("====================");
    println!();
    println!("Total completed: {}", report.total_completed);
    println!(
        "Streak: {} (longest: {})",
        report.current_streak, report.longest_streak
    );
    if let Some(next) = report.next_milestone {
        println!("Next milestone: {} tasks", next);
    } else {
        println!("Next milestone: (none)");
    }
    println!();

    if !report.milestones.is_empty() {
        println!("Milestones achieved:");
        for m in &report.milestones {
            let celebrated = if m.celebrated { "✓" } else { " " };
            println!(
                "  [{}] {} tasks at {}",
                celebrated, m.threshold, m.achieved_at
            );
        }
        println!();
    }

    if !report.recent_completions.is_empty() {
        println!("Recent completions:");
        for t in &report.recent_completions {
            println!("  {} - {} ({})", t.id, t.title, t.completed_at);
        }
    }
}

/// Print velocity report in text format
pub fn print_velocity_report_text(report: &ProductivityVelocityReport) {
    println!("Productivity Velocity ({} days)", report.window_days);
    println!("===============================");
    println!();
    println!("Total completed: {}", report.total_completed);
    println!("Average/day: {:.2}", report.average_per_day);
    if let Some((day, count)) = &report.best_day {
        println!("Best day: {} ({} tasks)", day, count);
    }
}

/// Print streak report in text format
pub fn print_streak_report_text(report: &ProductivityStreakReport) {
    println!("Productivity Streak");
    println!("===================");
    println!();
    println!("Current streak: {}", report.current_streak);
    println!("Longest streak: {}", report.longest_streak);
    println!(
        "Last completion: {}",
        report.last_completed_date.as_deref().unwrap_or("(none)")
    );
}

/// Print estimation report in text format
pub fn print_estimation_report_text(report: &ProductivityEstimationReport) {
    println!("Estimation Accuracy");
    println!("===================");
    println!();

    if report.tasks_analyzed == 0 {
        println!("No tasks with both estimated and actual minutes found.");
        println!("Complete tasks with estimation data to see accuracy metrics.");
        return;
    }

    println!("Tasks analyzed: {}", report.tasks_analyzed);
    println!();
    println!(
        "Average accuracy: {:.2}x (1.0 = perfect)",
        report.average_accuracy_ratio
    );
    println!("Median accuracy:  {:.2}x", report.median_accuracy_ratio);
    println!();
    println!("Within 25%: {:.1}% of estimates", report.within_25_percent);
    println!();
    println!(
        "Average absolute error: {:.1} minutes",
        report.average_absolute_error_minutes
    );
    println!();

    // Interpretation
    if report.average_accuracy_ratio < 0.9 {
        println!("Trend: Tend to overestimate (actual < estimated)");
    } else if report.average_accuracy_ratio > 1.1 {
        println!("Trend: Tend to underestimate (actual > estimated)");
    } else {
        println!("Trend: Good calibration");
    }
}

/// Format a duration in seconds to a human-readable string
pub fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        format!("{}d {}h", days, hours)
    }
}
