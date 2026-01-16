# Implementation Queue

## Queue
- [ ] RQ-0486 [code]: Let specs --print-prompt run without runner binaries. (ralph_tui/internal/specs/specs.go)
  - Evidence: Build verifies the runner and acquires a lock before checking PrintPrompt, so `ralph specs build --print-prompt` fails if codex/opencode is missing and takes a lock unnecessarily.
  - Plan: Short-circuit runner verification and lock acquisition when PrintPrompt is true; add tests.
- [ ] RQ-0487 [ui]: Bound specs preview renderer cache growth. (ralph_tui/internal/tui/specs_view.go)
  - Evidence: previewRenderers caches a renderer per width with no eviction; repeated resizes accumulate renderers and memory.
  - Plan: Add an LRU/size cap or clear the cache on resize/theme changes; add tests.
- [ ] RQ-0488 [code]: Validate specs templates and docs-specific pin files. (ralph_tui/internal/pin/pin.go, ralph_tui/internal/pin/init.go, ralph_tui/internal/pin/templates.go)
  - Evidence: ValidatePin ignores specs_builder.md; MissingFiles does not include specs_builder_docs.md, so docs projects can miss the template without error.
  - Plan: Extend validation/missing-file checks to include specs builder templates based on project type; update init/migrate flows and tests.
- [ ] RQ-0489 [ui]: Fix done-summary ordering mismatch with pin move-checked defaults. (ralph_tui/internal/tui/dashboard_view.go, ralph_tui/internal/pin/pin.go, ralph_tui/cmd/ralph/main.go)
  - Evidence: ReadDoneSummary assumes newest Done item is at the top, but `ralph pin move-checked` defaults to appending; the dashboard "Last done" becomes incorrect after CLI moves.
  - Plan: Default to prepend in CLI or update summary logic to compute the latest item regardless of ordering; align help text/tests.
- [ ] RQ-0490 [code]: Enforce indentation for queue item metadata bullets. (ralph_tui/internal/pin/pin.go)
  - Evidence: validateQueueItemLines checks Evidence/Plan by trimmed prefix only; unindented "- Evidence:" passes even though the format requires indented metadata, leading to ambiguous formatting.
  - Plan: Require Evidence/Plan and extra metadata bullets to be indented by two spaces; update fixtures and validation tests.

## Blocked

## Parking Lot
