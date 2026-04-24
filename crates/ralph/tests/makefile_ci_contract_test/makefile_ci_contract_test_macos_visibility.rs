//! macOS app visibility contract tests for CI-gated sources.
//!
//! Purpose:
//! - macOS app visibility contract tests for CI-gated sources.
//!
//! Responsibilities:
//! - Keep default macOS CI/build codepaths free of ad hoc foreground app activation calls.
//! - Guard the workspace-window anchor against regressing back to headed reveal APIs.
//!
//! Not handled here:
//! - Interactive UI-test bundles and test-only helper code.
//! - Runtime behavior outside the tracked app-target Swift sources.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Shared foreground activation is centralized in `RalphMacPresentationRuntime`.
//! - `apps/RalphMac/RalphMac/SettingsSmokeContractRunner.swift` is the only allowed app-target file to contain raw activation primitives.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::makefile_ci_contract_test_support::repo_root;

#[test]
fn test_workspace_window_anchor_uses_shared_presentation_runtime() -> Result<()> {
    let repo_root = repo_root()?;
    let anchor_path = repo_root.join("apps/RalphMac/RalphMac/WorkspaceWindowAnchor.swift");
    let anchor = std::fs::read_to_string(&anchor_path)
        .with_context(|| format!("read {}", anchor_path.display()))?;

    assert!(
        anchor.contains("RalphMacPresentationRuntime.reveal(window)"),
        "WorkspaceWindowAnchor should route workspace-window reveal through the shared presentation runtime"
    );
    assert!(
        !anchor.contains("NSApp.activate(ignoringOtherApps: true)"),
        "WorkspaceWindowAnchor must not activate the app directly; that causes visible CI blips"
    );
    assert!(
        !anchor.contains("window.makeKeyAndOrderFront(nil)"),
        "WorkspaceWindowAnchor must not bypass the shared presentation runtime with makeKeyAndOrderFront"
    );

    Ok(())
}

#[test]
fn test_raw_foreground_activation_is_centralized_in_presentation_runtime() -> Result<()> {
    let repo_root = repo_root()?;
    let app_root = repo_root.join("apps/RalphMac/RalphMac");
    let allowed_files = [app_root.join("SettingsSmokeContractRunner.swift")];
    let forbidden_patterns = [
        "NSApp.activate(ignoringOtherApps: true)",
        "window.makeKeyAndOrderFront(nil)",
    ];

    let mut violations = Vec::new();
    for path in swift_files_under(&app_root)? {
        if allowed_files.iter().any(|allowed| allowed == &path) {
            continue;
        }

        let content =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let matched: Vec<&str> = forbidden_patterns
            .iter()
            .copied()
            .filter(|pattern| content.contains(pattern))
            .collect();
        if !matched.is_empty() {
            violations.push(format!("{} -> {}", path.display(), matched.join(", ")));
        }
    }

    assert!(
        violations.is_empty(),
        "raw foreground activation should stay centralized in RalphMacPresentationRuntime only. Violations:\n{}",
        violations.join("\n")
    );

    Ok(())
}

fn swift_files_under(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_swift_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_swift_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(root).with_context(|| format!("read dir {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_swift_files(&path, files)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) == Some("swift") {
            files.push(path);
        }
    }

    Ok(())
}
