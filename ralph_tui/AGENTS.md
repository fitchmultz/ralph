# Repository Guidelines

## Project Structure & Module Organization
- `cmd/ralph/`: CLI entrypoint (Cobra) for the Ralph CLI/TUI.
- `internal/`: Application packages (`config`, `loop`, `pin`, `prompts`, `specs`, `tui`, etc.).
- `ralph_legacy/`: Standalone legacy prompts and scripts (see `ralph_legacy/legacy/`), not required by the TUI.
- Tests live alongside code as `*_test.go` (no separate test directory).
- Repo configuration defaults and runtime config live under `.ralph/` (e.g., `.ralph/ralph.json`, `.ralph/pin/`, `.ralph/cache/`).

## Build, Test, and Development Commands
- `go run ./cmd/ralph`: Run the CLI/TUI locally.
- `go run ./cmd/ralph --help`: List commands and flags.
- `go build ./cmd/ralph`: Build the CLI binary.
- `go test ./...`: Run all unit tests.
- `go test ./... -run TestName`: Run a single test by name.

## Coding Style & Naming Conventions
- Go version is pinned in `go.mod`.
- Use standard Go formatting: `go fmt ./...` (or `gofmt -w` for specific files).
- Keep packages lower-case, concise, and aligned to their domain (e.g., `internal/loop`).
- File names are lower_snake_case; tests use `*_test.go`.

## Testing Guidelines
- Tests use the standard Go `testing` package.
- Add/extend tests for new logic in the same package as the code under test.
- Favor table-driven tests for multiple scenarios.
- Use `go vet ./...` for baseline static checks alongside tests.

## Commit & Pull Request Guidelines
- Commit messages often include queue IDs: `RQ-####: <short summary>`.
- If work is not tied to a queue item, use a short imperative summary.
- PRs should include a clear summary, the commands run (e.g., `go test ./...`), and any relevant notes about TUI behavior changes.

## Configuration & Runtime Files
- Local repo config is stored in `.ralph/ralph.json`; pins live in `.ralph/pin/`.
- Use `ralph migrate` to move legacy pin files into the `.ralph/pin` layout when needed.
