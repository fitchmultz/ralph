//! Purpose: Facade for filesystem utility helpers used across Ralph.
//!
//! Responsibilities:
//! - Declare focused fsutil companion modules.
//! - Re-export the stable filesystem utility API used by crate callers.
//! - Keep fsutil regression coverage colocated with the fsutil module.
//!
//! Scope:
//! - Thin facade only; filesystem helper behavior lives in sibling companion modules.
//!
//! Usage:
//! - Used through `crate::fsutil::*` imports across queue, config, runtime, and integration code.
//! - Keeps the existing `crate::fsutil` surface stable while implementation is split.
//!
//! Invariants/Assumptions:
//! - Existing public and crate-internal fsutil imports remain valid without caller changes.
//! - Companion modules stay private; the facade owns the stable surface.

mod atomic;
mod paths;
mod safeguard;
mod temp;

#[cfg(test)]
mod tests;

pub use crate::constants::paths::RALPH_TEMP_PREFIX;
pub use atomic::write_atomic;
pub use paths::expand_tilde;
pub use safeguard::{safeguard_text_dump, safeguard_text_dump_redacted};
pub use temp::{
    cleanup_default_temp_dirs, cleanup_stale_temp_dirs, cleanup_stale_temp_entries,
    create_ralph_temp_dir, create_ralph_temp_file, ralph_temp_root,
};

pub(crate) use atomic::sync_dir_best_effort;
