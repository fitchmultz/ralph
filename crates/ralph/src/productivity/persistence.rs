//! Productivity stats persistence layer.
//!
//! Purpose:
//! - Productivity stats persistence layer.
//!
//! Responsibilities:
//! - Load and save productivity stats from/to the cache directory.
//!
//! Not handled here:
//! - Data structure definitions (see `super::types`).
//! - Business logic for updating stats (see `super::calculations`).
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Stats file is JSON with schema version for migrations.
//! - All operations are atomic (read-modify-write with temp file + rename).

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::constants::paths::STATS_FILENAME;

use super::types::ProductivityStats;

/// Load productivity stats from cache directory
pub fn load_productivity_stats(cache_dir: &Path) -> Result<ProductivityStats> {
    let path = cache_dir.join(STATS_FILENAME);

    if !path.exists() {
        return Ok(ProductivityStats::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read productivity stats from {}", path.display()))?;

    let stats: ProductivityStats = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse productivity stats from {}", path.display()))?;

    Ok(stats)
}

/// Save productivity stats to cache directory
pub fn save_productivity_stats(stats: &ProductivityStats, cache_dir: &Path) -> Result<()> {
    let path = cache_dir.join(STATS_FILENAME);

    // Ensure cache directory exists
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("Failed to create cache directory {}", cache_dir.display()))?;

    let content =
        serde_json::to_string_pretty(stats).context("Failed to serialize productivity stats")?;

    // Atomic write: write to temp file then rename
    let temp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&temp_path)
        .with_context(|| format!("Failed to create temp file {}", temp_path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write to temp file {}", temp_path.display()))?;
    file.flush()
        .with_context(|| format!("Failed to flush temp file {}", temp_path.display()))?;
    drop(file);

    fs::rename(&temp_path, &path)
        .with_context(|| format!("Failed to rename temp file to {}", path.display()))?;

    Ok(())
}
