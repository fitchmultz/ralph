# Implementation Done

## Done
- [x] RQ-0301 [ui]: Simplify focus and keybindings (Tab focus, Ctrl+T pin pane, E specs settings, F logs format). (ralph_tui/internal/tui/model.go, ralph_tui/internal/tui/keymap.go, ralph_tui/internal/tui/help_keymap.go, ralph_tui/internal/tui/pin_view.go)
  - Evidence: Focus model is unintuitive; Pin uses Tab internally, conflicting with global focus expectations; help text out of sync.
  - Plan: Make Tab toggle nav/content focus; auto-focus content after nav selection; move pin pane toggle to Ctrl+T; add E/F bindings and update help text; update render_contract_test + model_test.
- [x] RQ-0102 [docs]: Seed done item. (README.md)
  - Evidence: Example done entry.
  - Plan: Replace with real completed work.
