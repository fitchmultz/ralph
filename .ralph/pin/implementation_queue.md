# Implementation Queue

## Queue
- [x] RQ-0401 [code]: Relax overly aggressive redaction; make it configurable. (ralph_tui/internal/loop/logger.go, ralph_tui/internal/loop/line_writer.go, ralph_tui/internal/loop/loop.go, ralph_tui/internal/config/config.go, ralph_tui/internal/config/defaults.json, ralph_tui/internal/tui/config_editor.go)
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
