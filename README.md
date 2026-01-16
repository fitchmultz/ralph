# Ralph

Ralph is a tool for managing AI agent loops and pin operations.

## Project Structure

This repository is focused on the Go TUI/CLI:

- **[ralph_tui](./ralph_tui)**: The active Go TUI/CLI. All new work and feature development targets this path.

## Getting Started

Refer to the README in `ralph_tui/` for usage. Default pin files live under `.ralph/pin/`.

## Project Types

Ralph supports a configurable `project_type` to tune prompts and workflows:

- `code` (default): code-focused prompts.
- `docs`: documentation-focused prompts (doc maintenance, link checks, research synthesis).

Set it via `ralph init --project-type docs` or the config editor to persist in `.ralph/ralph.json`.
