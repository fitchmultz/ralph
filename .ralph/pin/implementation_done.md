# Implementation Done

## Done
- [x] RQ-0302 [code]: Keep log file open + add Close() + write startup log. (ralph_tui/internal/tui/logging.go, ralph_tui/internal/tui/model.go)
  - Evidence: Logger opens/closes per entry; log file can be empty until user action; logger Close not called on quit.
  - Plan: Maintain persistent file handle with rotation handling; add Close(); emit tui.start on boot; call Close() on quit; add logging_perf_contract_test.
- [x] RQ-0300 [ui]: Fix footer sizing + border correctness across views. (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/render.go, ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/tui/specs_view.go)
  - Evidence: Footer height off-by-one; viewport padding causes overflow; clamp truncates instead of sizing correctly; several Resize() paths skip small sizes.
  - Plan: Trim footer before measuring; remove viewport padding or account for it; replace clamp truncation with proper sizing; ensure Resize() updates even at small sizes; add border contract test.
- [x] RQ-0301 [ui]: Simplify focus and keybindings (Tab focus, Ctrl+T pin pane, E specs settings, F logs format). (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/keymap.go, ralph_tui/internal/tui/help_keymap.go, ralph_tui/internal/tui/pin_view.go)
  - Evidence: Focus model is unintuitive; Pin uses Tab internally, conflicting with global focus expectations; help text out of sync.
  - Plan: Make Tab toggle nav/content focus; auto-focus content after nav selection; move pin pane toggle to Ctrl+T; add E/F bindings and update help text; update render_contract_test + model_test.
