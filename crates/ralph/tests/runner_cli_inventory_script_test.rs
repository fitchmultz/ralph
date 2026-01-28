//! Integration tests for `scripts/runner_cli_inventory.sh`.
//!
//! Responsible for verifying that the inventory script produces the expected output
//! directory structure and exit codes when runner binaries are present or missing.
//!
//! Does NOT:
//! - Verify the real runner CLIs' flags or semantics (that is Phase 2 manual review)
//! - Require any real runner binaries to be installed
//!
//! Assumptions / invariants:
//! - Tests run on a Unix-like environment that can execute bash scripts
//! - Fake runner binaries on PATH are sufficient to validate behavior

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

mod test_support;

fn repo_root() -> Result<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .context("resolve repo root")
        .map(PathBuf::from)
}

fn inventory_script_path(repo_root: &Path) -> PathBuf {
    repo_root.join("scripts/runner_cli_inventory.sh")
}

fn fake_runner_script(runner: &str) -> String {
    // Define runner-specific subcommands for help discovery
    let commands_section = match runner {
        "codex" => r#"Commands:
  exec        Run Codex non-interactively
  exec resume Resume a previous session
  help        Print help
"#,
        "opencode" => r#"Commands:
  run         Run with a message
  help        Show help
"#,
        _ => "", // Other runners have no subcommands
    };

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

runner="{runner}"

if [[ "${{1:-}}" == "--version" ]]; then
  echo "${{runner}} 0.0.1"
  exit 0
fi

if [[ "${{1:-}}" == "-V" ]]; then
  echo "${{runner}} 0.0.1"
  exit 0
fi

if [[ "${{1:-}}" == "version" ]]; then
  echo "${{runner}} 0.0.1"
  exit 0
fi

if [[ "${{*: -1}}" == "--help" ]]; then
  echo "${{runner}} help: $*"
  if [[ -n "{commands}" ]]; then
    echo ""
    echo "{commands}"
  fi
  exit 0
fi

echo "${{runner}} invoked: $*"
"#,
        commands = commands_section
    )
}

fn prepend_path(dir: &Path) -> Result<std::ffi::OsString> {
    let mut paths: Vec<std::path::PathBuf> = vec![dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).context("join PATH")
}

#[test]
#[cfg(unix)]
fn inventory_succeeds_with_fake_runners() -> Result<()> {
    let repo_root = repo_root()?;
    let script_path = inventory_script_path(&repo_root);

    let temp_dir = TempDir::new().context("create temp dir")?;
    for runner in ["codex", "opencode", "gemini", "claude", "agent"] {
        test_support::create_fake_runner(temp_dir.path(), runner, &fake_runner_script(runner))
            .with_context(|| format!("create fake runner: {runner}"))?;
    }

    let out_dir = temp_dir.path().join("out");
    let status = Command::new(&script_path)
        .args(["--out", out_dir.to_string_lossy().as_ref()])
        .env("PATH", prepend_path(&temp_dir.path().join("bin"))?)
        .status()
        .context("run inventory script")?;
    anyhow::ensure!(status.success(), "inventory script should succeed");

    for runner in ["codex", "opencode", "gemini", "claude", "agent"] {
        let runner_dir = out_dir.join(runner);
        anyhow::ensure!(
            runner_dir.join("resolved_path.txt").exists(),
            "missing resolved_path.txt for {runner}"
        );
        anyhow::ensure!(
            runner_dir.join("version.txt").exists(),
            "missing version.txt for {runner}"
        );
        anyhow::ensure!(
            runner_dir.join("help.base.txt").exists(),
            "missing help.base.txt for {runner}"
        );
    }

    // Subcommands are auto-discovered from the help output
    anyhow::ensure!(
        out_dir.join("codex/help.exec.txt").exists(),
        "missing codex exec help capture"
    );
    anyhow::ensure!(
        out_dir.join("opencode/help.run.txt").exists(),
        "missing opencode run help capture"
    );
    // Check for consolidated files (new output format)
    anyhow::ensure!(
        out_dir.join("codex/codex.md").exists(),
        "missing codex consolidated markdown file"
    );
    anyhow::ensure!(
        out_dir.join("opencode/opencode.md").exists(),
        "missing opencode consolidated markdown file"
    );

    Ok(())
}

#[test]
#[cfg(unix)]
fn inventory_exits_nonzero_when_base_help_fails() -> Result<()> {
    let repo_root = repo_root()?;
    let script_path = inventory_script_path(&repo_root);

    let temp_dir = TempDir::new().context("create temp dir")?;
    for runner in ["codex", "opencode", "gemini", "claude", "agent"] {
        let mut script = fake_runner_script(runner);
        if runner == "gemini" {
            script = r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--help" ]]; then
  echo "gemini help failed" >&2
  exit 1
fi
echo "gemini ok"
"#
            .to_string();
        }
        test_support::create_fake_runner(temp_dir.path(), runner, &script)
            .with_context(|| format!("create fake runner: {runner}"))?;
    }

    let out_dir = temp_dir.path().join("out");
    let output = Command::new(&script_path)
        .args(["--out", out_dir.to_string_lossy().as_ref()])
        .env("PATH", prepend_path(&temp_dir.path().join("bin"))?)
        .output()
        .context("run inventory script")?;
    anyhow::ensure!(
        !output.status.success(),
        "inventory script should exit non-zero when base help fails"
    );

    let help_path = out_dir.join("gemini/help.base.txt");
    anyhow::ensure!(
        help_path.exists(),
        "expected gemini help capture file to exist"
    );
    let help_contents = std::fs::read_to_string(&help_path).context("read gemini help capture")?;
    anyhow::ensure!(
        help_contents.contains("=== ERROR: command failed"),
        "expected failure marker in help capture"
    );

    Ok(())
}
