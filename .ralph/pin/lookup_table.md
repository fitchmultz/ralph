# Lookup Table

| Area | Notes |
| --- | --- |
| pin | Default pin fixtures for the Ralph TUI/CLI. |
| cli | Cobra CLI entrypoint + subcommands live in `ralph_tui/cmd/ralph/` (start at `ralph_tui/cmd/ralph/main.go`). |
| paths | Repo root discovery and config path resolution live in `ralph_tui/internal/paths/`. |
| migrate | Legacy pin migration logic lives in `ralph_tui/internal/migrate/`. |
| version | Build-time version metadata lives in `ralph_tui/internal/version/`. |
| config | Config models and save/load logic for the Ralph TUI live in `ralph_tui/internal/config/`. |
| tui | Ralph TUI layout, keybindings, and view behavior live in `ralph_tui/internal/tui/`. |
| loop | Supervised loop runner, git helpers, and redaction live in `ralph_tui/internal/loop/`. |
| lockfile | Directory-based lock helper shared across Ralph (`ralph_tui/internal/lockfile/`). |
| fileutil | Atomic file write helpers shared across Ralph (`ralph_tui/internal/fileutil/`). |
| procgroup | Shared process-group cancellation helpers live in `ralph_tui/internal/procgroup/`. |
| redaction | Redaction modes and env key classification live in `ralph_tui/internal/redaction/`. |
| queueid | Shared queue item ID parsing lives in `ralph_tui/internal/queueid/`. |
| runnerargs | Shared runner argument parsing and reasoning-effort helpers live in `ralph_tui/internal/runnerargs/`. |
| streaming | Shared line-splitting helpers for log streaming live in `ralph_tui/internal/streaming/`. |
| prompts | Default worker/supervisor prompt templates live in `ralph_tui/internal/prompts/defaults/`. |
| specs | Specs builder, prompt filling, and runner invocation live in `ralph_tui/internal/specs/` and legacy specs templates in `ralph_legacy/specs/`. |
| testutil | Shared test helpers for process/runner behavior live in `ralph_tui/internal/testutil/`. |
| docs | Repository-level guidance lives in `README.md`, `AGENTS.md`, and `CLAUDE.md`. |
