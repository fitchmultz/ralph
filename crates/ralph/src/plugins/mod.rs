//! Plugin system for Ralph (runners + task processors).
//!
//! Responsibilities:
//! - Define plugin manifest contracts and validation.
//! - Discover plugin packages from global + project plugin directories.
//! - Provide a registry for resolving enabled plugins, runner binaries, and processor hooks.
//!
//! Not handled here:
//! - CLI Clap argument definitions (see `crate::cli::plugin`).
//! - Execution-phase orchestration (see `crate::commands::run`).
//! - Streaming JSON parsing (see `crate::runner::execution::process`).
//!
//! Invariants/assumptions:
//! - Plugins are discovered from:
//!   - Global:  ~/.config/ralph/plugins/<plugin_id>/plugin.json
//!   - Project: .ralph/plugins/<plugin_id>/plugin.json
//! - Project plugins override global plugins of the same id.
//! - Plugins are disabled unless enabled in config.
//! - Plugin executables are NOT sandboxed by Ralph; enabling a plugin is equivalent to trusting it.

pub(crate) mod discovery;
pub(crate) mod manifest;
pub(crate) mod registry;

pub(crate) const PLUGIN_API_VERSION: u32 = 1;
