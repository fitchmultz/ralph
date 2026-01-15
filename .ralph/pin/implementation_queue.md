# Implementation Queue

## Queue
- [ ] RQ-0425 [ui]: Improve Run Loop screen UX: show active queue item/progress, add jump-to-Pin/logs shortcuts, and reduce wasted space while running. (ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/model.go, ralph_tui/internal/loop/loop.go)
  - Evidence:
    - The Run Loop screen currently shows static override values + a log viewport, but not the active queue item ID/title (even though the loop runner knows `firstItem.ID` and `currentItemBlock`).
    - Switching screens via the left nav during runs is slow and space-inefficient (and you explicitly want better use of screen space during loop runs).
  - Plan:
    - Plumb structured "loop state" messages (active item ID/title, iteration count, mode) from `internal/loop` into the TUI so the loop screen can show real progress.
    - Add a hotkey to jump directly to Pin (and auto-select the active item) and/or to Logs.
    - Combine with nav collapse to provide a genuinely useful "run mode" layout while the loop is active.
- [ ] RQ-0426 [ui]: Make Config editor less confusing: show per-field source (default/global/repo/session/cli), simplify Save actions, and add 'reset layer/field' controls. (ralph_tui/internal/tui/config_editor.go, ralph_tui/internal/config/load.go, ralph_tui/internal/tui/help_keymap.go)
  - Evidence:
    - The config editor supports layers but does not show where each effective value came from (defaults vs global vs repo vs session/CLI), which makes it hard to reason about changes.
    - Saving requires selecting an "Action" and then toggling "Apply action" (two-step), which is easy to miss and feels unintuitive compared to typical config UIs.
  - Plan:
    - Add per-field/source indicators (even if approximate) so users can see which layer is providing each value.
    - Replace the "Apply action" toggle with clearer explicit actions (key-driven save + on-screen hints) and add reset operations (reset field, reset layer/session).
    - Add tests ensuring session overrides remain correct and UI reflects effective config + sources.
- [ ] RQ-0427 [code]: Improve git helper error reporting + surface failures in the UI/logs instead of swallowing details. (ralph_tui/internal/loop/git.go, ralph_tui/internal/loop/loop.go, ralph_tui/internal/tui/logs_view.go)
  - Evidence:
    - `ralph_tui/internal/loop/git.go` `CurrentBranch()` returns `fmt.Errorf("Unable to detect current git branch.")` without the underlying error or stderr, making failures opaque.
    - `CommitAll`, `CommitPaths`, and `Push` discard stdout/stderr, preventing useful diagnostics when git operations fail.
    - `AheadCount()` returns 0 on several errors (e.g., no upstream), which can silently change behavior and confuse users.
  - Plan:
    - Wrap git command failures with stderr/stdout tails and log them through the loop logger (with redaction where applicable).
    - Update loop failure reporting to include actionable details (command + concise output tail).
    - Add tests using a stubbed git backend or hermetic repo fixtures to validate error messages and behavior.

## Blocked

## Parking Lot
