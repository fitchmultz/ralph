# Implementation Queue

## Queue
- [ ] RQ-0480 [code]: Stabilize pin queue validation and auto-innovate accounting. (ralph_tui/internal/pin/pin.go, ralph_tui/internal/specs/specs.go, ralph_tui/cmd/ralph/main.go)
  - Evidence: ValidatePin errors when queue/done are empty; uncheckedQueueCount counts any "- [ ]" line (including indented metadata) and ignores checked items; ResolveInnovateDetails returns Effective=true even when auto-enable is disabled for empty/missing queue; pin next-id runs ValidatePin so fresh queues cannot allocate IDs.
  - Plan: Allow empty queue/done while still enforcing format when items exist; count only top-level queue items (checked+unchecked) in ## Queue; fix ResolveInnovateDetails to respect explicit/disabled settings; update pin/specs CLI/tests/fixtures.
- [ ] RQ-0481 [code]: Normalize runner values across config, CLI, and loop invocations. (ralph_tui/internal/config/load.go, ralph_tui/internal/config/config.go, ralph_tui/internal/loop/loop.go, ralph_tui/cmd/ralph/main.go)
  - Evidence: ValidRunner lowercases input but applyPartial stores raw values; loop.verifyRunner only matches exact "codex"/"opencode"; CLI flags pass raw values so mixed-case runners pass config validation but fail at runtime.
  - Plan: Normalize runner strings in config load/apply and CLI overrides; ensure loop/specs use normalized values; add tests for mixed-case inputs.
- [ ] RQ-0482 [ui]: Refresh dashboard repo status when working tree changes. (ralph_tui/internal/tui/repo_status.go, ralph_tui/internal/tui/dashboard_view.go)
  - Evidence: RepoStatusSampler only watches .git/HEAD and .git/index; untracked or unstaged edits do not change these, so cached status is reused and the dashboard can show stale dirty state.
  - Plan: Add a working-tree change signal (git status fingerprint or repo-root mtime) to the sampling signature; update throttling logic/tests.
- [ ] RQ-0483 [ui]: Make log format/filter controls accurate and clear stale tail output. (ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/key_hints.go)
  - Evidence: Filters and format toggles are advertised globally but only apply to debug logs; refreshTailedFile keeps old lines when a log file disappears, so the UI shows stale output after deletion/rotation.
  - Plan: Apply formatting/filters to loop/specs output or scope the controls/labels to debug-only; clear cached lines and errors when tailed files go missing; update help text and tests.
- [ ] RQ-0484 [code]: Prevent long log lines from being split mid-line. (ralph_tui/internal/streaming/line_splitter.go, ralph_tui/internal/loop/line_writer.go, ralph_tui/internal/tui/stream_writer.go)
  - Evidence: LineSplitter flushes partial buffers after DefaultMaxBufferedBytes (512), so long JSON/log lines are emitted as multiple lines and break formatting/filters.
  - Plan: Increase the default buffer, make it configurable, and/or flush only on newline boundaries; add tests for long-line integrity.
- [ ] RQ-0485 [code]: Make pin tag parsing and checkbox matching case-insensitive. (ralph_tui/internal/pin/pin.go, ralph_tui/internal/loop/queue.go)
  - Evidence: tagPattern only matches lowercase tags, so headers like [UI] are ignored and --only-tag filtering fails; queueItemLine only matches "- [x]" so uppercase "- [X]" items are skipped by validation and MoveCheckedToDone.
  - Plan: Use case-insensitive tag parsing and allow [X] in queue item detection; normalize tags/checked state consistently; add tests.
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
