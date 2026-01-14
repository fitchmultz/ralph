# Ralph

Entry points (Go CLI):
- `go run ./cmd/ralph` (from `ralph_tui/`)
- `ralph loop run`
- `ralph specs build`
- `ralph pin validate`
- `ralph migrate`

Pin files live in `.ralph/pin/`.
Prompts live in `ralph_legacy/prompt.md` and `ralph_legacy/prompt_opencode.md`.
Legacy shell/python scripts are archived under `ralph_legacy/bin/legacy/`.
Python dependencies for legacy scripts are managed via `uv` in `ralph_legacy/` (e.g., `uv sync --project ralph_legacy --dev`).
