# Contributor Guide

## Project Structure & Module Organization
- `ralph_tui/`: Active Go-based CLI/TUI (`go run ./cmd/ralph`). All new development targets this path; source lives in `internal/` with tests alongside as `*_test.go`.
- `ralph_legacy/`: Frozen, maintenance-only legacy shell/Python scripts (`bin/`) and prompt/spec templates (`specs/`). Only critical fixes land here.
- `.ralph/`: Runtime/config defaults, including pin files at `.ralph/pin/` and cache/config files.
- `README.md`: High-level orientation and links to component-specific docs.

## Build, Test, and Development Commands
- `make install`: Install Python deps via `uv` and download Go modules.
- `make build`: Build the Go CLI binary.
- `make test`: Run Go tests; runs Python tests if any exist under `ralph_legacy/`.
- `make format`: Format Python (Ruff) and Go (gofmt).
- `make lint`: Lint Python (Ruff) and Go (go vet).
- `make type-check`: Run Python type checks (Astral Ty) and a no-op Go test pass for type safety.
- `make pin-validate`: Validate pin files via `ralph pin validate`.
- `make ci`: Local gate; runs generate/format/type-check/lint/pin-validate/build/test.

## Coding Style & Naming Conventions
- Go: standard formatting (`gofmt`), lower_snake_case filenames, lower-case package names.
- Python: typed where practical; format/lint with Ruff; type-check with Astral Ty.
- Shell: scripts live in `ralph_legacy/bin/` and follow existing `snake_case.sh` naming.
- Prompts/specs are Markdown; keep them generalized (no project-specific assumptions).

## Testing Guidelines
- Go tests use the standard `testing` package and live alongside code as `*_test.go`.
- Legacy Python currently has no test suite; add tests under `ralph_legacy/tests/` and run with `pytest`.
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

## Agent Notes
- Default pin/spec templates live in `.ralph/pin/` (TUI) and `ralph_legacy/specs/` (legacy).
- Update path references in docs/prompts when moving or renaming directories.
- For TUI resize behavior, avoid min-size clamps that exceed available space; views should shrink to fit to prevent selection/highlight mismatches.
- Loop runner inactivity is controlled by `loop.runner_inactivity_seconds`; when triggered, the loop resets to the last known good commit and restarts the item (no WIP quarantine).
- TUI keybinding policy (RQ-0469):
  - Global actions must use `ctrl+` combos only; avoid bare letters for global scope.
  - While typing in a content view, global shortcuts do not fire (except quit); route keys to the active view.
  - Screen-specific letter bindings are only safe when not typing and must be reflected in help/hints and conflict tests.
