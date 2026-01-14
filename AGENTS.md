# Repository Guidelines

## Structure & Entry Points
- `ralph_legacy/`: Legacy scripts and prompt templates for the standalone legacy workflow (see `ralph_legacy/legacy/`).
- `ralph_tui/`: Go-based CLI/TUI (`go run ./cmd/ralph`) with embedded prompt defaults.
- Default pin/spec templates live in `.ralph/pin/` (TUI) and `ralph_legacy/specs/` (legacy).

## Local Verification
- Use `make ci` as the local gate before considering work complete.

## Docs & Prompts
- Keep prompt templates and pin/spec fixtures generalized (no project-specific assumptions).
- Update path references when moving or renaming directories.
