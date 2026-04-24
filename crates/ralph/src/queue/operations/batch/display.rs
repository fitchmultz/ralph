//! Batch operation result display utilities.
//!
//! Purpose:
//! - Batch operation result display utilities.
//!
//! Responsibilities:
//! - Print batch operation results in a user-friendly format.
//! - Handle both dry-run and actual execution output.
//! - Display created task IDs for operations that generate tasks.
//!
//! Non-scope:
//! - Actual batch operations (see update.rs, delete.rs, generate.rs, plan.rs).
//! - Result calculation or aggregation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - Output goes to stdout via println!.
//! - Dry run output clearly indicates no changes were made.

use super::BatchOperationResult;

/// Print batch operation results in a user-friendly format.
pub fn print_batch_results(result: &BatchOperationResult, operation_name: &str, dry_run: bool) {
    if dry_run {
        println!(
            "Dry run - would perform {} on {} tasks:",
            operation_name, result.total
        );
        for r in &result.results {
            if r.success {
                println!("  - {}: would update", r.task_id);
            } else {
                println!(
                    "  - {}: would fail - {}",
                    r.task_id,
                    r.error.as_deref().unwrap_or("unknown error")
                );
            }
        }
        println!("Dry run complete. No changes made.");
        return;
    }

    // Collect created task IDs for operations that create tasks
    let created_count: usize = result
        .results
        .iter()
        .map(|r| r.created_task_ids.len())
        .sum();

    if result.has_failures() {
        println!("{} completed with errors:", operation_name);
        for r in &result.results {
            if r.success {
                println!("  ✓ {}: updated", r.task_id);
                if !r.created_task_ids.is_empty() {
                    for created_id in &r.created_task_ids {
                        println!("    → Created: {}", created_id);
                    }
                }
            } else {
                println!(
                    "  ✗ {}: failed - {}",
                    r.task_id,
                    r.error.as_deref().unwrap_or("unknown error")
                );
            }
        }
        println!(
            "Completed: {}/{} tasks updated successfully.",
            result.succeeded, result.total
        );
        if created_count > 0 {
            println!("Created {} new tasks.", created_count);
        }
    } else {
        println!("{} completed successfully:", operation_name);
        for r in &result.results {
            println!("  ✓ {}", r.task_id);
            if !r.created_task_ids.is_empty() {
                for created_id in &r.created_task_ids {
                    println!("    → Created: {}", created_id);
                }
            }
        }
        println!("Updated {} tasks.", result.succeeded);
        if created_count > 0 {
            println!("Created {} new tasks.", created_count);
        }
    }
}
