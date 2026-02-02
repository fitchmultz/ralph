//! Command implementation layer.
//!
//! Purpose:
//! - Centralize "do the work" command implementations separate from the CLI surface (`crate::cli`).
//! - Mirror the CLI structure for easier navigation and maintenance.
//!
//! Non-goals:
//! - This module does not define clap argument parsing; see `crate::cli` for that.

pub mod context;
pub mod doctor;
pub mod init;
pub mod prd;
pub mod prompt;
pub mod run;
pub mod scan;
pub mod task;
pub mod watch;
