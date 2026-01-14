# Implementation Done

## Done
- [x] RQ-0305 [code]: Remove busy tick; add log batching with run scoping for loop output. (ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/stream_writer.go)
  - Evidence: loopView tickCmd wakes every 500ms while running; loop log channel drops lines when buffer is full; no run ID to ignore stale messages if a new run starts.
  - Plan: Replace tickCmd with log-only updates; introduce a batched log message helper with run IDs; drain log channel into batches to reduce UI churn; ignore stale run batches; add loop_view_log_batch_test.
- [x] RQ-0304 [ui]: Fix pin view layout and reload lifecycle. (ralph_tui/internal/tui/pin_view.go)
  - Evidence: No Pin header; detail viewport padding causes overflow; magic column widths; reloadAgain sticks on error.
  - Plan: Add header line; remove/size padding correctly; compute table widths from styles; clear reloadAgain on error/start; add pin_view_reload_again_test.
- [x] RQ-0303 [code]: Introduce fileStamp change detection and gate refreshes. (ralph_tui/internal/tui/file_watch.go, ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/model.go)
  - Evidence: fileChanged only uses modtime; Logs refresh runs unconditionally and re-reads entire file.
  - Plan: Replace modtime-only API with fileStamp (modtime+size+exists); use it to skip log tailing if unchanged; reduce refresh churn.
- [x] RQ-0302 [code]: Keep log file open + add Close() + write startup log. (ralph_tui/internal/tui/logging.go, ralph_tui/internal/tui/model.go)
  - Evidence: Logger opens/closes per entry; log file can be empty until user action; logger Close not called on quit.
  - Plan: Maintain persistent file handle with rotation handling; add Close(); emit tui.start on boot; call Close() on quit; add logging_perf_contract_test.
- [x] RQ-0300 [ui]: Fix footer sizing + border correctness across views. (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/render.go, ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/tui/specs_view.go)
  - Evidence: Footer height off-by-one; viewport padding causes overflow; clamp truncates instead of sizing correctly; several Resize() paths skip small sizes.
  - Plan: Trim footer before measuring; remove viewport padding or account for it; replace clamp truncation with proper sizing; ensure Resize() updates even at small sizes; add border contract test.
- [x] RQ-0301 [ui]: Simplify focus and keybindings (Tab focus, Ctrl+T pin pane, E specs settings, F logs format). (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/keymap.go, ralph_tui/internal/tui/help_keymap.go, ralph_tui/internal/tui/pin_view.go)
  - Evidence: Focus model is unintuitive; Pin uses Tab internally, conflicting with global focus expectations; help text out of sync.
  - Plan: Make Tab toggle nav/content focus; auto-focus content after nav selection; move pin pane toggle to Ctrl+T; add E/F bindings and update help text; update render_contract_test + model_test.
