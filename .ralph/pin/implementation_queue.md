# Implementation Queue

## Queue

- [ ] RQ-0473 [code]: Consolidate specs build option precedence across CLI + TUI; centralize innovate/autofill/scout/user-focus/runner args resolution. (ralph_tui/internal/specs/specs.go, ralph_tui/cmd/ralph/main.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/runnerargs/effort.go)
  - Evidence: Specs options are resolved in multiple places: CLI flags in `cmd/ralph/main.go`, TUI toggles in `specs_view.go`, and innovate auto-enable logic in `specs.ResolveInnovateDetails`. This duplication makes behavior inconsistent (e.g., config provides autofill/scout/user_focus but CLI flags default false and TUI has its own toggles), and runner-effort injection is duplicated.
  - Plan: Introduce a single resolver in `internal/specs/` that merges config + explicit toggles + CLI/session state into an "effective specs options" struct used by both CLI and TUI. Add unit tests for precedence and innovate auto-enable behavior.

- [ ] RQ-0474 [docs]: Fix docs project prompting: shift from "docs bug sweep" to "docs iteration/completion"; make innovate/scout instructions project-type aware. (ralph_tui/internal/specs/specs.go, ralph_tui/internal/prompts/defaults/specs_bug_sweep_docs.md, ralph_tui/internal/prompts/defaults/prompt_codex_docs.md, ralph_tui/internal/prompts/defaults/prompt_opencode_docs.md, .ralph/pin/specs_builder_docs.md)
  - Evidence: The docs bug sweep entry focuses on "broken/outdated links" and similar hygiene (`specs_bug_sweep_docs.md`) but doesn’t explicitly drive documentation completion/iteration; `specs.go` uses a single `innovateInstructions` block that is explicitly code/bug-hunt oriented and is applied to docs projects too, contributing to "docs treated like code."
  - Plan: Redesign docs prompts to explicitly cover doc iteration: fleshing out placeholders, restructuring sections, reconciling terminology, adding examples, and validating navigation/links. Implement project-type-specific innovate instructions and (if needed) scout workflow variants; update both embedded defaults and `.ralph/pin` templates.

- [ ] RQ-0475 [code]: Deduplicate prompt/template sources to prevent drift (pin defaults vs embedded prompts vs .ralph templates). (ralph_tui/internal/pin/defaults.go, ralph_tui/internal/pin/templates.go, ralph_tui/internal/prompts/prompts.go, ralph_tui/internal/prompts/defaults/*, .ralph/pin/specs_builder.md, .ralph/pin/specs_builder_docs.md)
  - Evidence: Prompt content is duplicated in multiple places: `.ralph/pin/specs_builder*.md`, embedded defaults in `internal/pin/defaults.go`, and embedded worker prompts in `internal/prompts/defaults/`. These copies are similar-but-not-identical, increasing drift and confusing users.
  - Plan: Introduce canonical prompt "partials" and generate/compose templates from a single source of truth. Ensure pin init/template creation and embedded defaults reference the same canonical content; add a consistency check (and/or tests) to catch drift.

- [ ] RQ-0476 [ui]: Pin screen feature completeness: add unblock/requeue actions and blocked-item tooling in TUI (stop relying on external editor for routine flows). (ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/tui/keymap.go, ralph_tui/internal/tui/help_keymap.go, ralph_tui/internal/pin/pin.go)
  - Evidence: The `pin` package supports `RequeueBlockedItem` and blocked metadata parsing (`pin.go`), but `pin_view.go` exposes no TUI action to requeue/unblock blocked items, inspect WIP branch metadata in an actionable way, or perform common pin workflows without opening an external editor.
  - Plan: Add blocked-item actions in the Pin view: requeue selected blocked item (top/bottom), show/copy WIP branch + known-good SHA, and optionally reset fixup attempt metadata. Add keybindings + help; keep commands disabled while loop is running.

- [ ] RQ-0477 [ui]: Logs screen improvements: use persisted loop/specs output on restart, harden tailing, and add lightweight filtering. (ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/file_watch.go)
  - Evidence: Loop/specs output is persisted to disk (`loop_output.log`, `specs_output.log`) but Logs view only displays in-memory buffers passed from active views, so after a restart the Logs screen loses loop/specs history even though files exist. Also, `tailFileLinesFromHandle` trims `\n` but not `\r`, and formatted mode repeatedly JSON-decodes log lines, which can be costly during frequent refreshes.
  - Plan: Teach Logs view to read persisted loop/specs outputs from cache as a fallback/source-of-truth; watch those files with the existing stamp logic. Normalize CRLF handling in tailing; add simple filters (component/level) and keep formatted rendering incremental/cached.

- [ ] RQ-0478 [code]: Config/path resolution + error reporting fixes (stop silently ignoring path resolution errors; surface actionable messages in UI). (ralph_tui/internal/config/load.go, ralph_tui/internal/config/config.go, ralph_tui/internal/tui/config_editor.go)
  - Evidence: `resolveConfigPaths` silently ignores `resolvePathWithRepo` errors (`config/load.go`), which can leave invalid (relative) paths that later fail `Config.Validate()` with less actionable messages. Config editor field-source refresh also ignores errors, hiding why values can’t be resolved.
  - Plan: Propagate path resolution errors with contextual messages (which field/path failed and why), surface them in the config editor status line, and add unit tests for `{repo}` expansion and relative-root save/load edge cases.

- [ ] RQ-0479 [ops]: Reduce refresh/jitter and background workload to address lag (adaptive refresh, debounce preview rendering, avoid heavy work when screen inactive). (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/repo_status.go)
  - Evidence: `refreshCmd` ticks frequently and triggers repo status sampling + view refresh checks even when screens are inactive (`model.refreshViews`). Specs preview rendering (glamour) can be expensive and is re-triggered on many resizes (`specs_view.Resize` sets `previewDirty=true`), contributing to a laggy experience.
  - Plan: Make refresh adaptive: only run heavy refresh logic when the relevant screen is visible, debounce preview rendering on rapid resize, and add lightweight timing logs at debug level to identify hotspots. Keep a manual "refresh now" as an escape hatch.

## Blocked

## Parking Lot
