//! Queue file loading facade with read-only semantics.
//!
//! Purpose:
//! - Queue file loading facade with read-only semantics.
//!
//! Responsibilities:
//! - Expose queue-file load helpers for plain reads, parse repair, and validation.
//! - Coordinate queue/done loading for read-only validation flows.
//! - Keep semantic repair writes in `queue::repair` so undo semantics are centralized.
//!
//! Not handled here:
//! - Queue file saving (see `queue::save`).
//! - ID generation or backup management.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Missing queue files return default empty queues.
//! - Pure load/validate APIs never write to disk.
//! - Mutating repair APIs live outside this loader facade.

mod read;
mod validation;

pub use read::{
    load_and_validate_queues, load_queue, load_queue_or_default, load_queue_with_repair,
    load_queue_with_repair_and_validate,
};

#[cfg(test)]
mod tests;
