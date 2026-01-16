# Contributor Guide

## Project Structure & Module Organization
- `ralph_tui/`: Active Go-based CLI/TUI (`go run ./cmd/ralph`). All new development targets this path; source lives in `internal/` with tests alongside as `*_test.go`.
- `.ralph/`: Runtime/config defaults, including pin files at `.ralph/pin/` and cache/config files.
- `README.md`: High-level orientation and links to component-specific docs.

## Build, Test, and Development Commands
- `make install`: Download Go modules.
- `make build`: Build the Go CLI binary.
- `make test`: Run Go tests.
- `make format`: Format Go (gofmt).
- `make lint`: Lint Go (go vet).
- `make type-check`: Run a no-op Go test pass for type safety.
- `make pin-validate`: Validate pin files via `ralph pin validate`.
- `make ci`: Local gate; runs generate/format/type-check/lint/pin-validate/build/test.

## Coding Style & Naming Conventions
- Go: standard formatting (`gofmt`), lower_snake_case filenames, lower-case package names.
- Prompts/specs are Markdown; keep them generalized (no project-specific assumptions).

## Testing Guidelines
- Go tests use the standard `testing` package and live alongside code as `*_test.go`.
- Prefer table-driven tests for multiple scenarios in Go.

## Co & Pull Request Guidelines
- Commit messages in history are sentence-case summaries; some include multiple sentences.
- No formal PR template; include:
  - A concise summary of changes.
  - Commands run (especially `make ci` or `go test ./...`).
  - Notes on prompt/spec changes or TUI behavior changes.
  - Screenshots or recordings if TUI UI behavior changes.

## Configuration & Security
- Prefer a single project-root `.env` if configuration is needed; keep `.env.example` in sync.
- Do not commit real secrets if the repo is public.

## First-Principles Simplicity
- Start from the fundamentals, strip to essentials, then rebuild the simplest working path (think SpaceX’s Raptor approach).
- Delete before adding: remove dead code, redundant layers, and stale comments; net-negative diffs are wins when behavior stays correct.
- Complexity budget: add components only when they reduce total risk/maintenance or increase measurable value.
- Evidence over opinion: tests, data constraints, and benchmarks settle debates; formatters/linters settle style.
- Centralize early: if similar logic exists, consolidate into shared helpers/modules.

## Agent Notes
- Default pin/spec templates live in `.ralph/pin/`.
- Update path references in docs/prompts when moving or renaming directories.
- For TUI resize behavior, avoid min-size clamps that exceed available space; views should shrink to fit to prevent selection/highlight mismatches.
- Loop runner inactivity is controlled by `loop.runner_inactivity_seconds`; when triggered, the loop resets to the last known good commit and restarts the item (no WIP quarantine).
- TUI keybinding policy (RQ-0469):
  - Global actions must use `ctrl+` combos only; avoid bare letters for global scope.
  - While typing in a content view, global shortcuts do not fire (except quit); route keys to the active view.
  - Screen-specific letter bindings are only safe when not typing and must be reflected in help/hints and conflict tests.
