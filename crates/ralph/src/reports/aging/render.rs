//! Text rendering helpers for aging reports.
//!
//! Purpose:
//! - Text rendering helpers for aging reports.
//!
//! Responsibilities:
//! - Render the human-readable `ralph queue aging` report.
//! - Keep presentation separate from bucket computation.
//!
//! Not handled here:
//! - JSON rendering.
//! - Aging calculation or threshold validation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Non-fresh buckets may include task lists; fresh/unknown lists are intentionally compact.

use super::model::AgingReport;

pub(super) fn print_text_report(report: &AgingReport) {
    println!("Task Aging Report");
    println!("=================");
    println!();

    println!(
        "Thresholds: warning > {}d, stale > {}d, rotten > {}d",
        report.thresholds.warning_days, report.thresholds.stale_days, report.thresholds.rotten_days
    );
    println!(
        "Filtering by status: {}",
        report.filters.statuses.join(", ")
    );
    println!();

    println!("Totals ({} tasks)", report.totals.total);
    println!("  Fresh:    {}", report.totals.fresh);
    if report.totals.warning > 0 {
        println!("  Warning:  {}", report.totals.warning);
    }
    if report.totals.stale > 0 {
        println!("  Stale:    {}", report.totals.stale);
    }
    if report.totals.rotten > 0 {
        println!("  Rotten:   {}", report.totals.rotten);
    }
    if report.totals.unknown > 0 {
        println!("  Unknown:  {}", report.totals.unknown);
    }
    println!();

    for bucket in &report.buckets {
        if bucket.bucket == "fresh" || bucket.tasks.is_empty() {
            continue;
        }

        let title = match bucket.bucket.as_str() {
            "rotten" => "🟥 Rotten Tasks",
            "stale" => "🟧 Stale Tasks",
            "warning" => "🟨 Warning Tasks",
            "unknown" => "❓ Unknown Age",
            _ => &bucket.bucket,
        };
        println!("{}", title);
        println!("{}", "-".repeat(title.len()));
        for task in &bucket.tasks {
            println!(
                "  {}  {:10}  {:12}  {}",
                task.id,
                task.status.as_str(),
                task.age_human,
                task.title
            );
        }
        println!();
    }

    if report.totals.total == 0 {
        println!("No tasks match the selected filters.");
    }
}
