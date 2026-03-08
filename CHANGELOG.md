# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `make release-verify` now prepares and records a publish-ready local snapshot under `target/release-verifications/`, and `make release` publishes only if that exact snapshot still matches `HEAD`, release metadata, release notes, and artifacts.

## [0.2.2] - 2026-03-08

### Added

- Durable watch-task identity metadata and reconciliation rules so scan/remove flows only mutate the files processed in the current batch.
- Atomic task mutation support for the macOS app through `ralph task mutate`, including optimistic locking and status-derived field updates in a single transaction path.
- Repo execution trust controls for project-local CI gate, runner override, and plugin execution settings.

### Changed

- Release automation now uses an explicit transaction workflow with `scripts/release.sh verify`, `execute`, and `reconcile`, transaction state under `target/release-transactions/`, and local-finalization-before-publication semantics.
- Public-readiness checks now scan the whole repository for markdown-link breakage, tracked runtime artifacts, tracked env files, and obvious secret material instead of relying on a hardcoded document subset.
- Agent CI routing now follows dependency surface changes instead of `apps/RalphMac/` path prefixes, escalating shared CLI/build/runtime contract changes to `macos-ci`.
- The macOS app, Makefile, and release artifact builder now share one CLI bundling/build entrypoint to keep app-bundled and shipped binaries on the same toolchain contract.
- Queue loading, managed subprocess execution, runner/runtime modules, and macOS app window/task presentation flows were refactored into smaller focused components for more predictable behavior and recovery.

### Security

- CI gate execution now rejects shell-string launchers and untrusted repo-local execution settings, and webhook failure diagnostics store only redacted destinations.

## [0.2.1] - 2026-03-06

## [0.2.0] - 2026-03-06

### Added

- macOS SwiftUI app (`apps/RalphMac/`) that drives Ralph by executing the bundled `ralph` CLI.
- `ralph app open` (macOS-only) to launch the installed app (bundle id: `com.mitchfultz.ralph`).
- Hidden GUI/tooling contract: `ralph __cli-spec --format json` (emitted from clap's command model).
- `ralph task decompose` to recursively plan task trees from a freeform goal or an existing queue task, preview the hierarchy, and write durable child tasks back into the queue.
- Dedicated decomposition prompt plumbing, queue-safe subtree materialization, optional sibling dependency inference, attach/replace child policies, and machine-readable preview/write output for automation.
- Full macOS app parity for task decomposition, including dedicated UI flows, toolbar/menu entry points, preview/write behavior, and regression coverage.

### Removed

- Rust terminal UI (`ralph tui`) and interactive `-i/--interactive` entrypoints.
- TUI-only dependencies (`ratatui`, `crossterm`, and related crates).

## [0.1.0] - 2026-01-27

### Added

- Initial release of Ralph, a Rust CLI for managing AI agent loops with a structured JSON task queue.
- Queue management: JSON-based task queue (`.ralph/queue.json`) with priority, status, and dependency tracking.
- Task lifecycle: Create, update, complete, reject, and archive tasks with automatic timestamp tracking.
- Multi-phase workflow: Configurable 1, 2, or 3-phase execution (planning → implementation → review).
- Runner integration: Support for Codex, OpenCode, Gemini, Claude, and Cursor CLIs.
- TUI interface: Interactive terminal UI for queue inspection and task management.
- Prompt system: Embedded prompt templates with per-repo override support.
- Configuration: Layered JSON config (global + project) with schema validation.
- RepoPrompt integration: Optional planning and tooling injection for RepoPrompt-enabled environments.
- Git integration: Automatic commit/push on task completion with configurable behavior.
- CI gate: Built-in `make macos-ci` validation pipeline (format, lint, type-check, test, build, install).
- Queue validation: Schema validation for queue and config files.
- Task scanning: Automatic task generation from codebase analysis.
- Lock management: File-based locking with stale lock detection and force options.

### Security

- Secure credential handling: Secrets redaction in logs and queue entries.
- Lock file isolation: Prevents concurrent queue modifications.

[Unreleased]: https://github.com/fitchmultz/ralph/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/fitchmultz/ralph/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/fitchmultz/ralph/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/fitchmultz/ralph/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fitchmultz/ralph/releases/tag/v0.1.0
