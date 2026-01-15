# Implementation Queue

## Queue
- [ ] RQ-0439 [code]: Fix reasoning_effort "auto" semantics and policy block accuracy (align what we display vs what we actually pass to codex; make P1 behavior explicit). (ralph_tui/internal/loop/loop.go, ralph_tui/internal/runnerargs/effort.go, ralph_tui/internal/tui/loop_view.go)
  - Evidence:
    - `loop.Run()` computes an `effectiveEffort` (including `[P1] => high`) and prints the "CODEX CONTEXT BUILDER POLICY" block based on it, but `runnerargs.ApplyReasoningEffort(..., "auto")` may inject no args—so the policy block can claim an effort that isn’t actually applied.
    - The Run Loop UI displays "effective" effort using a different path (`runnerargs.ApplyReasoningEffort`), so the UI and the prompt policy can disagree and confuse both users and agents.
  - Plan:
    - Use a single source of truth for "effective effort" based on the final runner args (post-merge + post-injection) and reuse it for both UI display and prompt policy output.
    - Decide on and implement the desired auto behavior for `[P1]` items (either actually inject `high` or show "auto (target: high)" consistently everywhere).
    - Add regression tests for policy block output and the P1 auto-effort behavior so we never misreport the active reasoning mode again.

## Blocked

## Parking Lot
