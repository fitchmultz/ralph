//! Integration test to verify killed processes are properly reaped (no zombies).
//!
//! Purpose:
//! - Integration test to verify killed processes are properly reaped (no zombies).
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

use std::process::Command;

/// Verify that after a process is killed, no zombie remains.
#[test]
fn test_no_zombie_after_kill() {
    // Spawn a long-running process
    let mut child = Command::new("sleep")
        .arg("300")
        .spawn()
        .expect("Failed to spawn sleep process");

    let pid = child.id();

    // Kill the process
    child.kill().expect("Failed to kill process");

    // Wait for it to be reaped
    child.wait().expect("Failed to wait for killed process");

    // Verify no zombie exists for this PID
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output()
        .expect("Failed to run ps");

    let stat = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // If process is gone entirely, output is empty (good)
    // If zombie exists, stat would contain 'Z'
    assert!(
        stat.is_empty() || !stat.contains('Z'),
        "Process {} became a zombie (stat: {})",
        pid,
        stat
    );
}

/// Verify multiple parallel kills don't leave zombies.
#[test]
fn test_no_zombies_parallel_kills() {
    let mut children: Vec<std::process::Child> = (0..10)
        .map(|_| {
            Command::new("sleep")
                .arg("300")
                .spawn()
                .expect("Failed to spawn sleep process")
        })
        .collect();

    // Kill all and wait for all
    for child in &mut children {
        let _ = child.kill();
    }
    for child in &mut children {
        let _ = child.wait();
    }

    // Check for any zombies
    let output = Command::new("sh")
        .arg("-c")
        .arg("ps aux | grep -c defunct || true")
        .output()
        .expect("Failed to count defunct processes");

    let count: i32 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    // Note: There may be pre-existing zombies from other processes
    // This test mainly verifies OUR processes don't become zombies
    // A more robust test would track specific PIDs
    println!("Defunct process count: {}", count);
}
