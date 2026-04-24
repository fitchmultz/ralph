//! Manual temp file cleanup command.
//!
//! Purpose:
//! - Manual temp file cleanup command.
//!
//! Responsibilities:
//! - Provide on-demand cleanup of ralph temporary files.
//! - Support dry-run mode for safe preview of cleanup actions.
//! - Support force mode for immediate cleanup regardless of age.
//!
//! Not handled here:
//! - Automatic cleanup (see main.rs startup cleanup and fsutil cleanup triggers).
//! - Tutorial sandbox cleanup (--keep-sandbox is intentional persistence).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Cleanup is best-effort and logs warnings on individual file errors.
//! - Dry-run lists what would be deleted without making changes.

use std::time::Duration;

use anyhow::Result;

use crate::cli::cleanup::CleanupArgs;
use crate::fsutil;

/// Run the cleanup command.
pub fn run(args: &CleanupArgs) -> Result<()> {
    let retention = if args.force {
        Duration::ZERO
    } else {
        crate::constants::timeouts::TEMP_RETENTION
    };

    if args.dry_run {
        return run_dry_run(retention);
    }

    let removed = fsutil::cleanup_default_temp_dirs(retention)?;
    println!("Cleaned up {} temp entries", removed);
    Ok(())
}

fn run_dry_run(retention: Duration) -> Result<()> {
    let base = fsutil::ralph_temp_root();
    println!(
        "Dry run - would clean temp files older than {:?}",
        retention
    );
    println!("Temp directory: {}", base.display());

    if !base.exists() {
        println!("Temp directory does not exist - nothing to clean.");
        return Ok(());
    }

    let entries = list_entries_to_clean(
        &base,
        &[crate::constants::paths::RALPH_TEMP_PREFIX],
        retention,
    )?;

    if entries.is_empty() {
        println!("No temp files or directories would be cleaned.");
    } else {
        println!("Would delete {} entries:", entries.len());
        for entry in entries {
            let entry_type = if entry.is_dir() { "[dir]" } else { "[file]" };
            println!("  {} {}", entry_type, entry.display());
        }
    }

    // Also check legacy prefix
    let legacy_entries = list_entries_to_clean(
        &std::env::temp_dir(),
        &[crate::constants::paths::LEGACY_PROMPT_PREFIX],
        retention,
    )?;

    if !legacy_entries.is_empty() {
        println!("\nWould delete {} legacy entries:", legacy_entries.len());
        for entry in legacy_entries {
            let entry_type = if entry.is_dir() { "[dir]" } else { "[file]" };
            println!("  {} {}", entry_type, entry.display());
        }
    }

    Ok(())
}

fn list_entries_to_clean(
    base: &std::path::Path,
    prefixes: &[&str],
    retention: Duration,
) -> Result<Vec<std::path::PathBuf>> {
    use std::fs;
    use std::time::SystemTime;

    let mut entries = Vec::new();

    if !base.exists() {
        return Ok(entries);
    }

    let now = SystemTime::now();

    for entry in fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        let modified = match metadata.modified() {
            Ok(time) => time,
            Err(_) => continue,
        };

        let age = match now.duration_since(modified) {
            Ok(age) => age,
            Err(_) => continue,
        };

        if age >= retention {
            entries.push(path);
        }
    }

    Ok(entries)
}
