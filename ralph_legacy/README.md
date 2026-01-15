# Ralph

This directory is frozen and maintenance-only. All new work and feature development targets
`ralph_tui/`.

Entry points (Go CLI):
- `go run ./cmd/ralph` (from `ralph_tui/`)
- `ralph loop run`
- `ralph specs build`
- `ralph pin validate`
- `ralph migrate`

Pin files live in `.ralph/pin/` for the TUI and `ralph_legacy/specs/` for the legacy scripts.
Prompts live in `ralph_legacy/prompt.md` and `ralph_legacy/prompt_opencode.md`.
Legacy shell/python scripts are archived under `ralph_legacy/bin/`.
Legacy spec templates live in `ralph_legacy/specs/`.
Python dependencies for legacy scripts are managed via `uv` in `ralph_legacy/` (e.g., `uv sync --project ralph_legacy --dev`).
