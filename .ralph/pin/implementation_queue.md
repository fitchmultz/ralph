# Implementation Queue

## Queue
- [ ] RQ-0489 [ui]: Fix done-summary ordering mismatch with pin move-checked defaults. (ralph_tui/internal/tui/dashboard_view.go, ralph_tui/internal/pin/pin.go, ralph_tui/cmd/ralph/main.go)
  - Evidence: ReadDoneSummary assumes newest Done item is at the top, but `ralph pin move-checked` defaults to appending; the dashboard "Last done" becomes incorrect after CLI moves.
  - Plan: Default to prepend in CLI or update summary logic to compute the latest item regardless of ordering; align help text/tests.
- [ ] RQ-0490 [code]: Enforce indentation for queue item metadata bullets. (ralph_tui/internal/pin/pin.go)
  - Evidence: validateQueueItemLines checks Evidence/Plan by trimmed prefix only; unindented "- Evidence:" passes even though the format requires indented metadata, leading to ambiguous formatting.
  - Plan: Require Evidence/Plan and extra metadata bullets to be indented by two spaces; update fixtures and validation tests.
- [ ] RQ-0491 [code]: Support docs-* and code-* routing tags in queue validation for the default pin file. (.ralph/pin/implementation_queue.md, ralph_tui/internal/pin/pin.go, ralph_tui/internal/pin/pin_test.go, ralph_tui/cmd/ralph/main.go)
  - Evidence: Queue validation only allows [db]/[ui]/[code]/[ops]/[docs]; tags like [docs-compliance] or [code-refactor] are rejected even when they reflect valid work streams.
  - Plan: Allow [docs-*] and [code-*] tags (any suffix) in addition to existing tags; update validation and CLI help examples; add tests for docs-* and code-* tags against the default pin file.
- [ ] RQ-0492 [ui]: Add a TUI task-builder mode (plus CLI entrypoint) that turns prompts into queue-formatted items. (ralph_tui/internal/tui, ralph_tui/internal/pin, ralph_tui/internal/prompts, ralph_tui/cmd/ralph/main.go)
  - Evidence: Users currently have to hand-format queue entries and track IDs/tags manually; the requested workflow is an agent-led prompt session that inserts correctly formatted tasks automatically.
  - Plan: Implement a TUI flow with a CLI entrypoint that launches it; agent choices are Codex/OpenCode with low/medium/high reasoning options; reuse NextQueueID/queue format rules; add prompt injection templates and repo recon for Evidence/Plan; add tests and help text.
- [ ] RQ-0493 [code]: Support an optional Notes metadata bullet in queue items. (ralph_tui/internal/pin/pin.go, ralph_tui/internal/pin/pin_test.go, ralph_tui/internal/pin/testdata/pin/implementation_queue.md)
  - Evidence: Queue validation currently only recognizes Evidence/Plan metadata; optional context needs are forced into Evidence/Plan or left out.
  - Plan: Extend metadata validation to allow a "- Notes:" bullet (optional); update fixtures and tests to cover items with and without Notes.
- [ ] RQ-0494 [ui]: Fix Build Specs "edit user focus" input and prompt injection. (ralph_tui/internal/tui, ralph_tui/internal/prompts, ralph_tui/internal/specs)
  - Evidence: Pressing "u" switches the panel to user focus edit mode, but typed input does not render or persist, and saving does not inject user focus into the prompt.
  - Plan: Trace the Build Specs view input handling for user focus; ensure the input component is bound to state, rendered, and persisted; update prompt assembly to include the user focus content; add regression tests for edit/save behavior.
- [ ] RQ-0495 [code]: Define a retention/trimming strategy for implementation_done.md to prevent unbounded growth. (ralph_tui/internal/pin, ralph_tui/internal/tui, .ralph/pin/implementation_done.md)
  - Evidence: implementation_done.md grows indefinitely as tasks are completed, which risks slower reads and TUI load times.
  - Plan: Measure current read/parse behavior; propose and document a safe retention limit and trimming strategy; add tooling or automation to enforce it; update tests and any UX messaging as needed.

## Blocked

## Parking Lot
