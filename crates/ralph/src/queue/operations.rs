//! Task queue task-level operations.
//!
//! This module contains operations that mutate or query tasks within queue files,
//! such as completing tasks, setting statuses/fields, finding tasks, deleting tasks,
//! and sorting tasks by priority. Persistence helpers (load/save/locks/repair) live
//! in `crate::queue` and are called from here when needed.

mod archive;
mod edit;
mod fields;
mod mutation;
mod query;
mod status;
mod validate;

pub use archive::*;
pub use edit::*;
pub use fields::*;
pub use mutation::*;
pub use query::*;
pub use status::*;

#[cfg(test)]
#[path = "operations/tests/mod.rs"]
mod tests;
