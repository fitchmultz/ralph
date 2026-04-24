//! Integration test hub for queue list/sort ordering behavior.
//!
//! Purpose:
//! - Integration test hub for queue list/sort ordering behavior.
//!
//! Responsibilities:
//! - Group queue sorting coverage by CLI validation, list ordering, persistent sort mutations, and dry-run output.
//! - Keep queue fixture builders in adjacent suite-local support layered on shared integration helpers.
//! - Preserve the historical `mod test_support` access for common integration helpers.
//!
//! Not handled here:
//! - Queue validation runtime tests inside `src/queue/`.
//! - Other queue CLI behaviors unrelated to sorting.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Sorting fixtures are serialized through shared task builders instead of inline JSON blobs.
//! - Task ID assertions read either tab-separated `queue list` output or the persisted queue file.

mod test_support;

#[path = "queue_sorting_cli_test/queue_sorting_cli_test_dry_run.rs"]
mod queue_sorting_cli_test_dry_run;
#[path = "queue_sorting_cli_test/queue_sorting_cli_test_list.rs"]
mod queue_sorting_cli_test_list;
#[path = "queue_sorting_cli_test/queue_sorting_cli_test_sort.rs"]
mod queue_sorting_cli_test_sort;
#[path = "queue_sorting_cli_test/queue_sorting_cli_test_support.rs"]
mod queue_sorting_cli_test_support;
#[path = "queue_sorting_cli_test/queue_sorting_cli_test_validation.rs"]
mod queue_sorting_cli_test_validation;
