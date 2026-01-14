# Implementation Queue

## Queue
- [ ] RQ-0302 [code]: Keep log file open + add Close() + write startup log. (ralph_tui/internal/tui/logging.go, ralph_tui/internal/tui/model.go)
  - Evidence: Logger opens/closes per entry; log file can be empty until user action; logger Close not called on quit.
  - Plan: Maintain persistent file handle with rotation handling; add Close(); emit tui.start on boot; call Close() on quit; add logging_perf_contract_test.
- [ ] RQ-0303 [code]: Introduce fileStamp change detection and gate refreshes. (ralph_tui/internal/tui/file_watch.go, ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/model.go)
  - Evidence: fileChanged only uses modtime; Logs refresh runs unconditionally and re-reads entire file.
  - Plan: Replace modtime-only API with fileStamp (modtime+size+exists); use it to skip log tailing if unchanged; reduce refresh churn.
- [ ] RQ-0304 [ui]: Fix pin view layout and reload lifecycle. (ralph_tui/internal/tui/pin_view.go)
  - Evidence: No Pin header; detail viewport padding causes overflow; magic column widths; reloadAgain sticks on error.
  - Plan: Add header line; remove/size padding correctly; compute table widths from styles; clear reloadAgain on error/start; add pin_view_reload_again_test.
- [ ] RQ-0305 [code]: Add run IDs + batched log streaming; remove busy tick. (ralph_tui/internal/tui/async_lines.go, ralph_tui/internal/tui/loop_view.go)
  - Evidence: tickCmd wakes every 500ms; log channels can drop lines; stale messages can corrupt current run.
  - Plan: Add runID + listenLineBatches helper; batch log lines; ignore stale run messages; remove tick loop; add loop_view_async_test.
- [ ] RQ-0306 [code]: Prevent overlapping preview renders; batch run output. (ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/async_lines.go)
  - Evidence: preview renders can overlap; run output updates are O(n^2).
  - Plan: Add previewLoading/previewDirty gating; schedule follow-up render if dirty; batch run logs via listenLineBatches; add specs_view_preview_queue_test.
- [ ] RQ-0307 [ui]: Expose runner/args/effort for Loop and Specs; parse args lines. (ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/args_text.go)
  - Evidence: Runner hard-coded to codex; no UI for args or effort; Specs lacks settings.
  - Plan: Add settings fields for runner, args (one per line), and reasoning effort; parse/format helpers; use settings when building commands; update help text.
- [ ] RQ-0308 [ui]: Make Logs screen readable and optionally raw. (ralph_tui/internal/tui/logs_view.go)
  - Evidence: JSONL rendered as raw JSON; refresh runs every tick.
  - Plan: Format JSONL into readable lines; add raw/formatted toggle; only refresh when fileStamp changes; keep status line showing resolved log path.

## Blocked

- [ ] RQ-0300 [ui]: Fix footer sizing + border correctness across views. (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/render.go, ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/tui/specs_view.go)
  - Evidence: Footer height off-by-one; viewport padding causes overflow; clamp truncates instead of sizing correctly; several Resize() paths skip small sizes.
  - Plan: Trim footer before measuring; remove viewport padding or account for it; replace clamp truncation with proper sizing; ensure Resize() updates even at small sizes; add border contract test.
  - Blocked reason: Supervisor runner failed.
  - Blocked reason: Unblock: Inspect ralph/wip/RQ-0300/20260114_135241 and requeue once fixed.
  - WIP branch: ralph/wip/RQ-0300/20260114_135241
  - Known-good: 3d12c22e422e25e4c6e9799caeae38d314f9b044
  - Unblock hint: Inspect ralph/wip/RQ-0300/20260114_135241 and requeue once fixed.
## Parking Lot
