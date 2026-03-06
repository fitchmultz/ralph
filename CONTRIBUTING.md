# Contributing to Ralph

Purpose: Guide for contributing to Ralph, covering development workflow, standards, and submission process.

Thank you for your interest in contributing to Ralph! This document provides guidelines for contributing effectively.

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (toolchain pinned via `rust-toolchain.toml`)
- GNU Make >= 4 (`make` on Linux, `gmake` on many macOS setups)
- Git

### Installation from Source

```bash
# Clone the repository
git clone https://github.com/mitchfultz/ralph
cd ralph

# Install locally
make install
```

This installs the `ralph` binary to `~/.local/bin/ralph` (or a writable fallback).

> macOS note: install GNU Make with `brew install make` and use `gmake ...` unless your PATH already points `make` to Homebrew gnubin.

### Your First Contribution (Suggested Path)

A low-risk first contribution loop:

```bash
# 1) Read orientation docs
# (README + docs/index.md)

# 2) Create a focused branch
git checkout -b RQ-XXXX-first-contribution

# 3) Make one small change + tests/docs

# 4) Run local gate
make agent-ci

# 5) Open a PR with verification notes
```

For docs-only work, still run `make agent-ci` to ensure formatting/lint/tests stay green.

## Development Workflow

### Local Development Cycle

During development, you can use these commands for rapid iteration:

```bash
# Run tests for the ralph crate
cargo test -p ralph-agent-loop

# Run the CLI locally
cargo run -p ralph-agent-loop -- <command>

# Validate the queue format
cargo run -p ralph-agent-loop -- queue validate

# List queue contents
cargo run -p ralph-agent-loop -- queue list

# Generate rustdocs for API review
make docs
```

### The CI Gate

**All contributions MUST pass `make agent-ci` before being considered complete.** This is a hard requirement.

`make agent-ci` behavior:

- Non-app changes: runs fast deterministic Rust/CLI gate (`make ci-fast`)
- App changes under `apps/RalphMac/`: escalates to `make macos-ci`

Fast gate (`ci-fast`) pipeline:

```
check-env-safety → check-backup-artifacts → deps → format → type-check → lint → test
```

Full Rust release gate (`ci`) adds:

```
build → generate → install
```

Canonical full `make ci` pipeline:

```
check-env-safety → check-backup-artifacts → deps → format → type-check → lint → test → build → generate → install
```

Run required gate with:

```bash
make agent-ci
# Optional (shared workstation): RALPH_CI_JOBS=4 make agent-ci
```

Do not commit or push changes if `make agent-ci` is failing. Fix all issues first.

### Fast Hygiene Checks (Before Commit)

For quick local verification before a full CI run:

```bash
make pre-commit
```

This runs environment safety checks, backup-artifact checks, and formatting validation.

For public-release verification:

```bash
make pre-public-check
# Optional (shared workstation): RALPH_CI_JOBS=4 RALPH_XCODE_JOBS=4 make pre-public-check
```

## Contribution Guidelines

### Code Standards

We follow standard Rust conventions with additional project-specific requirements:

- **Formatting**: `cargo fmt` (enforced by CI)
- **Linting**: `cargo clippy --workspace --all-targets -- -D warnings` (warnings treated as errors)
- **Visibility**: Default to private; prefer `pub(crate)` over `pub` unless cross-crate use is required
- **Errors**: Use descriptive error types (`thiserror`) and `Result<T, E>` over panics
- **Cohesion**: Keep modules/files focused; split large files rather than growing grab-bags

### Module Documentation

Every new or changed source file MUST start with a module doc comment that states:

- What the file is responsible for
- What it explicitly does NOT handle
- Any invariants/assumptions callers must respect

In Rust, prefer `//!` module docs at the top of the file:

```rust
//! Task queue management.
//!
//! Responsibility: Reading, writing, and validating the queue JSON file.
//! Does NOT handle: Task execution (see runner module) or git operations.
//! Invariants: Queue file must be valid JSON matching the queue schema.
```

### Testing Requirements

All new or changed behavior must be covered by tests:

- **Success modes**: Normal operation paths
- **Failure modes**: Error handling and edge cases
- **Location**: Prefer tests near the code via `#[cfg(test)]`
- **Integration tests**: Use `crates/ralph/tests/` for cross-module behavior

Example:

```bash
# Run all tests (nextest workspace tests with cargo-test fallback, then doc tests)
make test

# Run tests for just the ralph crate
cargo test -p ralph-agent-loop
```

### Code Coverage

Ralph uses `cargo-llvm-cov` for code coverage analysis. Coverage is **optional** and not part of the default CI gate.

#### Prerequisites

```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# On macOS, you may also need the llvm-tools component
rustup component add llvm-tools-preview
```

#### Running Coverage

```bash
# Generate coverage report
make coverage

# Clean coverage artifacts
make coverage-clean
```

The coverage target generates:
- **HTML Report**: `target/coverage/html/index.html` (opens automatically on macOS)
- **JSON Data**: `target/coverage/coverage.json` (machine-readable coverage data)

#### Interpreting Results

The HTML report shows:
- Line-by-line coverage highlighting
- Per-file coverage percentages
- Per-crate breakdown

The terminal output shows:
- Total coverage (lines, functions, regions)
- Per-crate breakdown (sorted alphabetically)

Coverage helps identify untested code paths but does not replace thoughtful test design.

For troubleshooting coverage issues, see [Troubleshooting](docs/troubleshooting.md).

### Integration Testing (CLI)

Ralph's CLI is a user-facing contract. For cross-module behaviors (argument parsing → filesystem IO → queue mutation → output),
prefer integration tests in `crates/ralph/tests/`.

#### Pattern: Isolated temp repo + CLI invocation

Use `crates/ralph/tests/test_support.rs` helpers to avoid repeating boilerplate:

- `temp_dir_outside_repo()` to isolate state
- `git_init(dir)` and `ralph_init(dir)` to create a valid repo
- `run_in_dir(dir, args)` to execute the compiled `ralph` binary
- `write_queue(...)` / `write_done(...)` and `read_queue()` / `read_done()` to set fixtures and assert results

Example skeleton:

```rust
let dir = test_support::temp_dir_outside_repo();
test_support::git_init(dir.path())?;
test_support::ralph_init(dir.path())?;

test_support::write_queue(dir.path(), &tasks)?;
let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "archive"]);
anyhow::ensure!(status.success(), "...\nstdout:\n{stdout}\nstderr:\n{stderr}");

let queue = test_support::read_queue(dir.path())?;
```

#### Snapshot testing with `insta`

For human-readable outputs that should remain stable (e.g., `queue graph`, `queue burndown`), we use `insta` snapshots.
Tests bind stable settings via `test_support::with_insta_settings(...)`, which normalizes newlines, strips ANSI, and replaces
date strings with `<DATE>` to prevent daily churn.

To update snapshots after an intentional output change:

```bash
INSTA_UPDATE=always cargo test -p ralph-agent-loop
```

Commit the updated snapshot files under `crates/ralph/tests/snapshots/`.

#### Isolation / flake prevention

- Always run `ralph init` with `--non-interactive` in tests.
- Prefer state assertions (queue/done JSON) for mutation commands.
- If a CLI output order is nondeterministic, fix determinism in the renderer (preferred) or strengthen snapshot filters (fallback).

### Feature Parity

When changing user-visible workflows, maintain parity between the CLI and the macOS app, or document/justify the divergence explicitly.

### CLI Help Documentation

User-facing commands and flags MUST have `--help` text with examples. Keep `docs/cli.md` in sync with changes.

Verify help text before committing:

```bash
cargo run -p ralph-agent-loop -- <command> --help
```

## Submitting Changes

### Commit Message Format

Preferred format: `RQ-####: <short summary>`

Where `####` is the task ID from `.ralph/queue.json`.

If no task ID exists (for example, first external contribution), use:

- `chore: <short summary>`
- `fix: <short summary>`
- `docs: <short summary>`

Examples:
- `RQ-0042: Add CI schema validation`
- `RQ-0007: Fix queue archive race condition`
- `docs: clarify run-loop troubleshooting step`

### Pull Request Expectations

Include in your PR description:

1. **What changed**: A brief summary of the changes
2. **How to verify**: Steps to validate (expected: `make agent-ci`)
3. **Breaking changes**: Call out any breaking behavior explicitly

Example:

````markdown
## Summary
Added validation for task ID format in queue operations.

## Verification
```bash
make agent-ci
```

## Breaking Changes
None.
````

### Local-CI-First Philosophy

This repository is local-CI-first. We avoid adding remote CI (e.g., GitHub Actions) as a substitute for `make agent-ci`. The local CI gate is the source of truth.

### Public Release Readiness

Before opening broad public visibility, run the dedicated checklist:

- [Public Readiness Checklist](docs/guides/public-readiness.md)

At minimum:

```bash
git status --short
git log --oneline -n 40
make agent-ci
```

If app changes are included in the release branch:

```bash
make macos-ci
# Optional caps while multitasking: RALPH_CI_JOBS=4 RALPH_XCODE_JOBS=4 make macos-ci
```

## Repository Structure

Key locations to know:

- `apps/RalphMac/`: macOS SwiftUI app (thin client that shells out to the bundled `ralph` CLI)
- `crates/ralph/`: Primary Rust CLI crate
  - `src/`: CLI commands, runner integration, queue management
  - `assets/prompts/`: Embedded prompt templates
- `docs/`: CLI + workflow + configuration docs (`docs/index.md` is the entry point)
- `schemas/`: Generated JSON schemas (committed)
- `.ralph/`: Repo-local runtime state
  - `queue.jsonc` (`.json` fallback): Active tasks (source of truth)
  - `done.jsonc` (`.json` fallback): Archived tasks
  - `config.jsonc` (`.json` fallback): Project config (overrides global)

## Where to Get Help

- **Fast-path guidelines**: See [AGENTS.md](./AGENTS.md) for quick reference
- **Detailed documentation**: See [docs/index.md](./docs/index.md)
- **Workflow details**: See [docs/workflow.md](./docs/workflow.md)
- **Configuration**: See [docs/configuration.md](./docs/configuration.md)
- **Security**: See [SECURITY.md](./SECURITY.md)

## Questions?

If you have questions not covered here:

1. Check the existing documentation in `docs/`
2. Review [AGENTS.md](./AGENTS.md) for contributor expectations
3. Open an issue for discussion before investing significant effort

By contributing to Ralph, you agree that your contributions are licensed under the project's MIT License.

Thank you for contributing to Ralph!
