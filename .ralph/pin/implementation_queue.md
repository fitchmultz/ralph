# Implementation Queue

## Queue
- [ ] RQ-0305 [code]: Remove busy tick; add log batching with run scoping for loop output. (ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/stream_writer.go)
  - Evidence: loopView tickCmd wakes every 500ms while running; loop log channel drops lines when buffer is full; no run ID to ignore stale messages if a new run starts.
  - Plan: Replace tickCmd with log-only updates; introduce a batched log message helper with run IDs; drain log channel into batches to reduce UI churn; ignore stale run batches; add loop_view_log_batch_test.
- [ ] RQ-0306 [code]: Coalesce preview refreshes and batch specs run output. (ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/stream_writer.go)
  - Evidence: refreshPreviewAsync can be triggered while previewLoading is true; rapid toggles can overlap goroutines; run output updates rebuild the log viewport for every line.
  - Plan: Gate preview refresh if loading and set previewDirty for a follow-up pass; add a single-flight refresh with queued rerender; batch run log writes via streamWriter batching; add specs_view_preview_queue_test.
- [ ] RQ-0307 [ui]: Expose runner/args/effort for Loop and Specs; parse args lines. (ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/config_editor.go, ralph_tui/internal/config/config.go)
  - Evidence: Loop runner is hard-coded to "codex" with empty args; Specs view defaults to codex with empty args; no UI for setting runner args or reasoning effort.
  - Plan: Add config-backed settings for runner, args (one per line), and reasoning effort; add parse/format helpers in TUI; wire through loop/specs builders; update settings help text.
- [ ] RQ-0308 [ui]: Make Logs screen readable and optionally raw. (ralph_tui/internal/tui/logs_view.go)
  - Evidence: Logs format toggle exists but renderContent always returns raw JSONL; no formatted view is applied to debug/loop/specs sections.
  - Plan: Parse JSONL entries into concise, human-readable lines; keep raw/formatted toggle; preserve status line with resolved log path.
- [ ] RQ-0400 [code]: Improve TUI observability and add an integration test harness to reduce manual debugging. (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/logging.go, ralph_tui/internal/tui/*_test.go)
  - Evidence: Users report there is no usable debug signal for agents; the log path is not obvious (and tui.start logs cfg.Logging.File, which is often blank when using default resolution); loop/specs output is only held in in-memory slices; several refresh/file errors are silently ignored (return nil), making failures invisible.
  - Plan: Log and display the resolved log path (not just cfg.Logging.File); persist loop/specs run output to cache files so agents can inspect after exit; plumb refresh/file errors into screen status plus debug log; add a model driver test helper to simulate WindowSizeMsg and key flows (start/stop loop, run specs, toggle logs format) and assert View() fits within bounds.
- [ ] RQ-0401 [code]: Relax overly aggressive redaction; make it configurable. (ralph_tui/internal/loop/logger.go, ralph_tui/internal/loop/line_writer.go, ralph_tui/internal/loop/loop.go, ralph_tui/internal/config/config.go, ralph_tui/internal/config/defaults.json, ralph_tui/internal/tui/config_editor.go)
  - Evidence: loop.Redactor redacts all environment values (>=4 chars) and key=value patterns; this hides non-secret paths and makes logs hard to use; user does not require strict env redaction.
  - Plan: Add a redaction mode setting (off, secrets_only, all_env) with secrets_only as default; only redact env keys that look sensitive by name; avoid redacting common path-like vars by default; add unit tests covering each mode.
- [ ] RQ-0402 [code]: Preserve CLI/session overrides on config reload; reflect effective config in editor. (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/config_editor.go, ralph_tui/internal/config/load.go)
  - Evidence: reloadConfigCmd calls LoadFromLocations without SessionOverrides/CLIOverrides; config editor reconstructs from files/defaults instead of active runtime config, so session changes are lost or invisible.
  - Plan: Track session overrides in model; pass both SessionOverrides and CLIOverrides into LoadFromLocations on reload; seed config editor from the effective config instead of file-only layers; add regression tests for reload preserving overrides.
- [ ] RQ-0403 [ui]: Fix Pin queue editing UX (toggle checked) + exact ID matching for block. (ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/pin/pin.go, ralph_tui/internal/pin/pin_test.go)
  - Evidence: pin.BlockItem matches via strings.Contains (RQ-0001 can match RQ-00010); Pin view has no toggle for checkmarks, forcing manual markdown edits.
  - Plan: Match item IDs exactly (use ExtractItemID or parsed QueueItem IDs); add toggle-checked action in Pin view; add unit tests for ID matching and toggle behavior.
- [ ] RQ-0404 [code]: Reduce log refresh churn and large-log stutter in Logs view. (ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/file_watch.go)
  - Evidence: tailFileLines uses os.ReadFile on every refresh; Refresh always rebuilds viewport content; large logs stutter and churn CPU.
  - Plan: Implement tailing that reads only the last N lines without loading full file; track rendered signature and skip SetContent when no content changes; add tests covering unchanged-stamp refresh.
- [ ] RQ-0405 [ui]: Fix global Tab focus stealing from huh forms (config/loop edit/pin block). (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/config_editor.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/pin_view.go)
  - Evidence: model handles Tab focus before delegating to active view; huh uses Tab for navigation, so forms can feel broken/unintuitive.
  - Plan: Detect active huh forms and bypass global focus toggle; or rebind focus key to avoid Tab conflict; update help text and add tests for form navigation.

## Blocked

## Parking Lot
