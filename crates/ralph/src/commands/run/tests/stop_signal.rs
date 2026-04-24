//! Stop signal tests for run command.
//!
//! Purpose:
//! - Stop signal tests for run command.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use crate::signal;

#[test]
fn stop_signal_is_detected_after_task_completion() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // Create stop signal
    signal::create_stop_signal(&cache_dir)?;
    assert!(signal::stop_signal_exists(&cache_dir));

    // Clear it
    let cleared = signal::clear_stop_signal(&cache_dir)?;
    assert!(cleared);
    assert!(!signal::stop_signal_exists(&cache_dir));

    Ok(())
}

#[test]
fn stop_signal_clear_is_idempotent() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // Clearing non-existent signal returns Ok(false)
    let cleared = signal::clear_stop_signal(&cache_dir)?;
    assert!(!cleared);

    Ok(())
}

#[test]
fn stop_signal_create_is_idempotent() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // First creation
    signal::create_stop_signal(&cache_dir)?;
    assert!(signal::stop_signal_exists(&cache_dir));

    // Second creation (should succeed, overwriting)
    signal::create_stop_signal(&cache_dir)?;
    assert!(signal::stop_signal_exists(&cache_dir));

    Ok(())
}

/// Test that stop signal is cleared when honored on loop exit
#[test]
fn stop_signal_cleared_on_sequential_loop_exit() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // Create stop signal
    signal::create_stop_signal(&cache_dir)?;
    assert!(signal::stop_signal_exists(&cache_dir));

    // Clear it (simulating what the loop does on exit)
    let cleared = signal::clear_stop_signal(&cache_dir)?;
    assert!(cleared);
    assert!(!signal::stop_signal_exists(&cache_dir));

    Ok(())
}

/// Test that stop signal clearing is idempotent (no error if already cleared)
#[test]
fn stop_signal_clear_is_idempotent_for_loop_exit() -> anyhow::Result<()> {
    let temp = tempfile::TempDir::new()?;
    let cache_dir = temp.path().join(".ralph/cache");

    // Clear non-existent signal should return Ok(false)
    let cleared = signal::clear_stop_signal(&cache_dir)?;
    assert!(!cleared);

    Ok(())
}
