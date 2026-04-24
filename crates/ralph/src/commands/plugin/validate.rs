//! Plugin validate command.
//!
//! Purpose:
//! - Plugin validate command.
//!
//! Responsibilities:
//! - Re-validate discovered plugin manifests.
//! - Confirm declared runner/processor executables resolve inside the plugin directory.
//!
//! Not handled here:
//! - Plugin installation, uninstall, or scaffolding.
//! - Registry enablement decisions.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Discovery already performed baseline manifest parsing before this command runs.
//! - Validation must fail fast on missing referenced executables.

use anyhow::{Context, Result, bail};

use crate::config::Resolved;
use crate::plugins::discovery::discover_plugins;
use crate::plugins::registry::resolve_plugin_relative_exe;

pub(super) fn run_validate(resolved: &Resolved, filter_id: Option<&str>) -> Result<()> {
    let discovered = discover_plugins(&resolved.repo_root)?;

    if discovered.is_empty() {
        println!("No plugins to validate.");
        return Ok(());
    }

    let mut validated = 0;
    let mut errors = 0;

    for (id, discovered_plugin) in discovered.iter() {
        if let Some(filter) = filter_id
            && id != filter
        {
            continue;
        }

        print!("Validating {}... ", id);

        if let Err(error) = discovered_plugin.manifest.validate() {
            println!("FAILED (manifest)");
            println!("  Error: {}", error);
            errors += 1;
            continue;
        }

        if let Some(runner) = &discovered_plugin.manifest.runner {
            let bin_path = resolve_plugin_relative_exe(&discovered_plugin.plugin_dir, &runner.bin)
                .context("resolve runner binary path")?;
            if !bin_path.exists() {
                println!("FAILED (runner binary)");
                println!("  Error: runner binary not found: {}", bin_path.display());
                errors += 1;
                continue;
            }
        }

        if let Some(processor) = &discovered_plugin.manifest.processors {
            let bin_path =
                resolve_plugin_relative_exe(&discovered_plugin.plugin_dir, &processor.bin)
                    .context("resolve processor binary path")?;
            if !bin_path.exists() {
                println!("FAILED (processor binary)");
                println!(
                    "  Error: processor binary not found: {}",
                    bin_path.display()
                );
                errors += 1;
                continue;
            }
        }

        println!("OK");
        validated += 1;
    }

    if let Some(filter) = filter_id
        && validated == 0
        && errors == 0
    {
        println!("Plugin '{}' not found.", filter);
    }

    if errors > 0 {
        bail!("{} validation error(s) found", errors);
    }

    println!("{} plugin(s) validated successfully.", validated);
    Ok(())
}
