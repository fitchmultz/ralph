//! Text rendering helpers for stats reports.
//!
//! Purpose:
//! - Text rendering helpers for stats reports.
//!
//! Responsibilities:
//! - Render the human-readable `ralph queue stats` text output.
//! - Keep CLI presentation separate from metrics computation.
//!
//! Not handled here:
//! - JSON rendering.
//! - Stats calculation or ETA lookup.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Rendering consumes a fully-populated `StatsReport`.

use super::model::StatsReport;

pub(super) fn print_text_report(
    report: &StatsReport,
    queue_file_size_kb: u64,
    show_eta_fallback: bool,
) {
    if report.summary.total == 0 {
        println!("No tasks found.");
        return;
    }

    println!("Task Statistics");
    println!("================");
    println!();

    println!("Total tasks: {}", report.summary.total);
    println!(
        "Terminal (done/rejected): {} ({:.1}%)",
        report.summary.terminal, report.summary.terminal_rate
    );
    println!("Done: {}", report.summary.done);
    println!("Rejected: {}", report.summary.rejected);
    println!("Active: {}", report.summary.active);
    println!("Queue file size: {}KB", queue_file_size_kb);
    println!();

    if let Some(durations) = &report.durations {
        println!(
            "Lead Time (created -> completed) for {} terminal task{}:",
            durations.count,
            if durations.count == 1 { "" } else { "s" }
        );
        println!("  Average: {}", durations.average_human);
        println!("  Median:  {}", durations.median_human);
        println!();
    }

    if let Some(work_time) = &report.time_tracking.work_time {
        println!(
            "Work Time (started -> completed) for {} terminal task{}:",
            work_time.count,
            if work_time.count == 1 { "" } else { "s" }
        );
        println!("  Average: {}", work_time.average_human);
        println!("  Median:  {}", work_time.median_human);
        println!();
    }

    if let Some(start_lag) = &report.time_tracking.start_lag {
        println!(
            "Start Lag (created -> started) for {} task{}:",
            start_lag.count,
            if start_lag.count == 1 { "" } else { "s" }
        );
        println!("  Average: {}", start_lag.average_human);
        println!("  Median:  {}", start_lag.median_human);
        println!();
    }

    if !report.velocity.by_tag.is_empty() {
        println!("Velocity by Tag (7d / 30d):");
        for entry in report.velocity.by_tag.iter().take(10) {
            println!(
                "  {}: {} / {}",
                entry.key, entry.last_7_days, entry.last_30_days
            );
        }
        println!();
    }

    if !report.velocity.by_runner.is_empty() {
        println!("Velocity by Runner (7d / 30d):");
        for entry in &report.velocity.by_runner {
            println!(
                "  {}: {} / {}",
                entry.key, entry.last_7_days, entry.last_30_days
            );
        }
        println!();
    }

    if !report.slow_groups.by_tag.is_empty() {
        println!("Slow Task Types by Tag (median work time):");
        for entry in report.slow_groups.by_tag.iter().take(5) {
            println!(
                "  {}: {} ({} tasks)",
                entry.key, entry.median_human, entry.count
            );
        }
        println!();
    }

    if !report.tag_breakdown.is_empty() {
        println!("Tag Breakdown:");
        for entry in &report.tag_breakdown {
            println!(
                "  {} ({}: {:.1}%)",
                entry.count, entry.tag, entry.percentage
            );
        }
        println!();
    }

    if let Some(eta) = &report.execution_history_eta {
        println!(
            "Execution History ETA (runner={}, model={}, phases={}):",
            eta.runner, eta.model, eta.phase_count
        );
        println!("  Samples: {}", eta.sample_count);
        println!(
            "  Estimated new task: {} (confidence: {})",
            eta.estimated_total_human, eta.confidence
        );
    } else if show_eta_fallback {
        println!("Execution History ETA: n/a (no samples for current runner/model/phases)");
    }
}
