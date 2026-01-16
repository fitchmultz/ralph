# Implementation Queue

## Queue
- [ ] RQ-0493 [code]: Support an optional Notes metadata bullet in queue items. (ralph_tui/internal/pin/pin.go, ralph_tui/internal/pin/pin_test.go, ralph_tui/internal/pin/testdata/pin/implementation_queue.md)
  - Evidence: Queue validation currently only recognizes Evidence/Plan metadata; optional context needs are forced into Evidence/Plan or left out.
  - Plan: Extend metadata validation to allow a "- Notes:" bullet (optional); update fixtures and tests to cover items with and without Notes.
- [ ] RQ-0494 [ui]: Fix Build Specs "edit user focus" input and prompt injection. (ralph_tui/internal/tui, ralph_tui/internal/prompts, ralph_tui/internal/specs)
  - Evidence: Pressing "u" switches the panel to user focus edit mode, but typed input does not render or persist, and saving does not inject user focus into the prompt.
  - Plan: Trace the Build Specs view input handling for user focus; ensure the input component is bound to state, rendered, and persisted; update prompt assembly to include the user focus content; add regression tests for edit/save behavior.
- [ ] RQ-0495 [code]: Define a retention/trimming strategy for implementation_done.md to prevent unbounded growth. (ralph_tui/internal/pin, ralph_tui/internal/tui, .ralph/pin/implementation_done.md)
  - Evidence: implementation_done.md grows indefinitely as tasks are completed, which risks slower reads and TUI load times.
  - Plan: Measure current read/parse behavior; propose and document a safe retention limit and trimming strategy; add tooling or automation to enforce it; update tests and any UX messaging as needed.
- [ ] RQ-0496 [code]: Change default ralph data directory to PWD .ralph/data instead of PWD data/. (ralph_tui/internal/config, ralph_tui/cmd/ralph/main.go)
  - Evidence: Current default data directory is `data/` at project root; should be `.ralph/data/` to keep all ralph-related files under `.ralph/`.
  - Plan: Update default data directory path from `data/` to `.ralph/data/` in configuration; update any path references and tests; ensure directory creation logic handles the new path correctly.
- [ ] RQ-0497 [code]: Change default log path to PWD .ralph/logs/ralph.log. (ralph_tui/internal/config, ralph_tui/cmd/ralph/main.go)
  - Evidence: Current default log path needs to be updated to `.ralph/logs/ralph.log` to keep all ralph-related files under `.ralph/`.
  - Plan: Update default log path to `.ralph/logs/ralph.log` in configuration; ensure logs directory is created if needed; update any path references and tests.


## Blocked

## Parking Lot
