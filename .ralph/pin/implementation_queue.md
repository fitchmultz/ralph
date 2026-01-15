# Implementation Queue

## Queue
- [ ] RQ-0460 [ops]: Make lockfile PID checks and specs lock temp dir portable (Windows-friendly). (ralph_tui/internal/lockfile/lockfile.go, ralph_tui/internal/specs/specs.go)
  - Evidence: `ralph_tui/internal/lockfile/lockfile.go` relies on `ps` for PID checks (`isPIDRunning`, `parentPID`), which is not portable to Windows; `specs.AcquireLock` falls back to hard-coded `/tmp` instead of `os.TempDir()`.
  - Plan: Split lockfile PID logic by platform (build-tagged implementations), switch specs lock base to `os.TempDir()` (and keep TMPDIR override), and add/adjust tests so `go test ./...` remains cross-platform.

- [ ] RQ-0452 [docs]: Align on-screen key hints and help output with actual bindings (remove misleading hints, document new shortcuts, add guard tests). (ralph_tui/internal/tui/dashboard_view.go, ralph_tui/internal/tui/help_keymap.go, ralph_tui/internal/tui/keymap.go)
  - Evidence: Several screens embed hard-coded "Keys:" lines that can drift from `keymap.go` (e.g., Dashboard advertises fixup blocked). This breaks discoverability and causes user confusion.
  - Plan: Centralize key-hint rendering (or derive from `keyMap`), update view strings, and add tests that assert advertised keys exist and are handled.

- [ ] RQ-0464 [code][ui][docs]: Add configurable project type (default: code; option: docs) to drive prompts and workflows for non-code repos. (ralph_tui/internal/config, ralph_tui/internal/tui/config_editor.go, ralph_tui/internal/prompts/defaults/, ralph_tui/internal/specs, ralph_tui/internal/loop, ralph_tui/internal/pin, README.md)
  - Evidence: Current prompt templates assume code-heavy repos, which performs poorly on doc-heavy knowledge bases; users need a docs-focused flow for documentation improvements, link fixes, and research synthesis.
  - Plan: Add `project_type` config (default `code`, allow `docs`) and persist it in pin/config; surface it in the config editor; during `ralph init`, prompt for repo type (with optional auto-detect + confirmation) so new repos start with the right prompts; select prompt templates for specs and loop runs based on project type; add docs-focused prompt variants (doc maintenance, link checks, research synthesis) and tests to ensure prompt selection + config round-trips per type.

## Blocked

## Parking Lot
