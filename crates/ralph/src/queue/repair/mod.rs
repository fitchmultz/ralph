//! Purpose: Facade for queue repair surfaces.
//!
//! Responsibilities:
//! - Declare focused repair companion modules.
//! - Re-export the stable repair API used by queue callers.
//! - Document the split between data, planning, persistence/apply, relationship
//!   rewriting, and dependency traversal helpers.
//!
//! Scope:
//! - Thin module root only; repair report/plan types, planning, apply,
//!   relationship rewriting, and dependency traversal live in sibling companions.
//!
//! Usage:
//! - Used through `crate::queue::{RepairReport, QueueRepairPlan,
//!   apply_queue_repair_with_undo, apply_queue_maintenance_repair_with_undo,
//!   plan_queue_repair, plan_queue_maintenance_repair, plan_loaded_queue_repair,
//!   get_dependents}`.
//! - Crate-internal helpers remain available through `crate::queue::repair::*`.
//!
//! Invariants/Assumptions:
//! - Re-exports preserve existing caller imports.
//! - Mutating repair flows live in `apply` and continue to require a held queue
//!   lock plus an undo snapshot before saving; planning flows are pure.

mod apply;
mod dependents;
mod planning;
mod relationships;
mod types;

pub use apply::{apply_queue_maintenance_repair_with_undo, apply_queue_repair_with_undo};
pub use dependents::get_dependents;
pub use planning::{plan_loaded_queue_repair, plan_queue_maintenance_repair, plan_queue_repair};
pub use types::{QueueRepairPlan, RepairReport};
