# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- macOS SwiftUI app (`apps/RalphMac/`) that drives Ralph by executing the bundled `ralph` CLI.
- `ralph app open` (macOS-only) to launch the installed app (bundle id: `com.mitchfultz.ralph`).
- Hidden GUI/tooling contract: `ralph __cli-spec --format json` (emitted from clap's command model).

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

[Unreleased]: https://github.com/fitchmultz/ralph/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/fitchmultz/ralph/releases/tag/v0.1.0
