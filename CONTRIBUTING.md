# Contributing to Ralph

Purpose: Guide for contributing to Ralph, covering development workflow, standards, and submission process.

Thank you for your interest in contributing to Ralph! This document provides guidelines for contributing effectively.

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- Make
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

## Development Workflow

### Local Development Cycle

During development, you can use these commands for rapid iteration:

```bash
# Run tests for the ralph crate
cargo test -p ralph

# Run the CLI locally
cargo run -p ralph -- <command>

# Validate the queue format
cargo run -p ralph -- queue validate

# List queue contents
cargo run -p ralph -- queue list
```

### The CI Gate

**All contributions MUST pass `make ci` before being considered complete.** This is a hard requirement.

The CI gate runs the full validation pipeline:

```
check-env-safety → check-backup-artifacts → generate → format → type-check → lint → build → test → install
```

Run it with:

```bash
make ci
```

Do not commit or push changes if `make ci` is failing. Fix all issues first.

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
# Run all tests
make test

# Run tests for just the ralph crate
cargo test -p ralph
```

### Feature Parity

When changing user-visible workflows, maintain parity between CLI and TUI, or document/justify the divergence explicitly.

### CLI Help Documentation

User-facing commands and flags MUST have `--help` text with examples. Keep `docs/cli.md` in sync with changes.

Verify help text before committing:

```bash
cargo run -p ralph -- <command> --help
```

## Submitting Changes

### Commit Message Format

Use the format: `RQ-####: <short summary>`

Where `####` is the task ID from `.ralph/queue.json`.

Examples:
- `RQ-0042: Add CI schema validation`
- `RQ-0007: Fix queue archive race condition`

### Pull Request Expectations

Include in your PR description:

1. **What changed**: A brief summary of the changes
2. **How to verify**: Steps to validate (expected: `make ci`)
3. **Breaking changes**: Call out any breaking behavior explicitly

Example:

```markdown
## Summary
Added validation for task ID format in queue operations.

## Verification
```bash
make ci
```

## Breaking Changes
None.
```

### Local-CI-First Philosophy

This repository is local-CI-first. We avoid adding remote CI (e.g., GitHub Actions) as a substitute for `make ci`. The local CI gate is the source of truth.

## Repository Structure

Key locations to know:

- `crates/ralph/`: Primary Rust CLI crate
  - `src/`: CLI commands, runner integration, queue management, TUI
  - `assets/prompts/`: Embedded prompt templates
- `docs/`: CLI + workflow + configuration docs (`docs/index.md` is the entry point)
- `schemas/`: Generated JSON schemas (committed)
- `.ralph/`: Repo-local runtime state
  - `queue.json`: Active tasks (source of truth)
  - `done.json`: Archived tasks
  - `config.json`: Project config (overrides global)

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

Thank you for contributing to Ralph!
