# Implementation Queue

## Queue
- [ ] RQ-0448 [code]: Harden Logs view tail reader against concurrent writes/rotations (avoid spurious errors and blank Logs screen). (ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/logs_view_test.go)
  - Evidence: `tailFileLines` does Stat → Seek → `io.ReadFull` across assumed-stable byte ranges; when the log file changes concurrently (append/rotate), this can return errors and surface as "Error:" in Logs.
  - Plan: Make tail reading resilient (ReadAt, tolerate EOF/UnexpectedEOF, retry on size changes), add a regression test that simulates concurrent append/rotation, and ensure the Logs screen degrades gracefully.

- [ ] RQ-0447 [code]: Improve loop/specs output persistence (buffering + fewer flushes) to avoid laggy UI while keeping lossless capture guarantees. (ralph_tui/internal/tui/output_persistence.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/output_capture_test.go)
  - Evidence: `outputFileWriter.AppendLines` flushes on every call; loop/specs write frequently, which can introduce IO stalls and degrade perceived streaming performance.
  - Plan: Add buffered/periodic flushing (or larger buffered batches), ensure Close always flushes, keep lossless test coverage, and add a benchmark/contract test to keep per-line overhead bounded.

- [ ] RQ-0451 [code]: Reduce Dashboard repo-status sampling cost (avoid running many git commands every refresh tick); add caching/throttling and better error surfacing. (ralph_tui/internal/tui/repo_status.go, ralph_tui/internal/tui/model.go)
  - Evidence: `repoStatusCmd` runs every UI refresh (default 5s) and executes several git commands sequentially (`CurrentBranch`, `ShortHeadSHA`, `StatusSummary`, `AheadCount`, `LastCommitSummary`, `LastCommitDiffStat`), which can become expensive and contribute to UI lag.
  - Plan: Cache repo status for a longer interval, consolidate git calls where possible, and add tests/benchmarks to keep dashboard refresh time bounded.

- [ ] RQ-0446 [code]: Reduce file-watch disk IO by hashing small files only when needed (size/modtime/inode unchanged) while preserving same-size/modtime content-change detection. (ralph_tui/internal/tui/file_watch.go, ralph_tui/internal/tui/file_watch_test.go)
  - Evidence: `getFileStamp` hashes contents for any file ≤64KB every time it is stamped, even when modtime/size are unchanged; this causes repeated disk reads on every refresh tick.
  - Plan: Implement a two-phase stamp compare (stat first; only hash when stat matches prior stamp but change detection needs it), update tests (including the "same size + same modtime" case), and confirm the TUI remains responsive on frequent refresh.

- [ ] RQ-0445 [code]: Fix specs preview performance lag by removing full log output strings from preview signature hashing; use stable versions/hashes instead. (ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/log_line_buffer.go, ralph_tui/internal/tui/render_perf_test.go)
  - Evidence: `previewInputSignature` incorporates `lastRunOutput` and `diffStat` directly, so signature computation becomes O(size of output) and can lag after large runs.
  - Plan: Replace signature inputs with cheap stable identifiers (e.g., log buffer version + diffStat hash/len), keep correctness (skip render only when truly unchanged), and add a perf/regression test to prevent reintroducing O(n) signature work.

- [ ] RQ-0449 [ui]: Close Pin screen feature gaps: provide access to blocked items, add prepend/append choice for Move Checked, and improve status/affordances. (ralph_tui/internal/tui/pin_view.go, ralph_tui/internal/pin/pin.go, ralph_tui/internal/tui/pin_view_test.go)
  - Evidence: Pin view shows `blockedCount` but provides no way to inspect blocked items from the UI; Move Checked always appends to Done (CLI supports `--prepend`), and status messages don't guide the user to next actions.
  - Plan: Add blocked-item browsing and move-checked options (default to prepend for recency), update key hints/help, and add tests covering the new UX paths.

- [ ] RQ-0444 [ui]: Make Run Loop settings UX runner-aware (opencode vs codex): hide/disable irrelevant reasoning-effort + context_builder controls; clarify behavior in the view. (ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/loop/loop.go, ralph_tui/internal/prompts/defaults/prompt_opencode.md)
  - Evidence: `loopView.controlsView` shows reasoning effort "effective: n/a" for non-codex runners and allows toggling "Force context_builder" even though the code-only context builder policy block is codex-specific; this is confusing, especially when using opencode.
  - Plan: Make the Run Loop screen adapt its controls/help text based on the selected runner, and add tests to ensure the view does not present no-op toggles or misleading "mandatory" labels.

- [ ] RQ-0461 [code]: Don't silently drop TUI logs: capture write/rotation errors and surface them in Logs status. (ralph_tui/internal/tui/logging.go, ralph_tui/internal/tui/logs_view.go, ralph_tui/internal/tui/logging_test.go)
  - Evidence: `tuiLogger.Log()` ignores file write errors (`written, _ := l.file.Write(...)`) and still increments `fileSize`, so disk-full/permission failures can silently drop logs and leave users with no diagnostics.
  - Plan: Make logger writes error-aware (capture and expose last error, close/reopen safely, and surface in `logsView.statusLine()`); add tests that simulate write failures and assert the error becomes visible and logging recovers when possible.

- [ ] RQ-0460 [ops]: Make lockfile PID checks and specs lock temp dir portable (Windows-friendly). (ralph_tui/internal/lockfile/lockfile.go, ralph_tui/internal/specs/specs.go)
  - Evidence: `ralph_tui/internal/lockfile/lockfile.go` relies on `ps` for PID checks (`isPIDRunning`, `parentPID`), which is not portable to Windows; `specs.AcquireLock` falls back to hard-coded `/tmp` instead of `os.TempDir()`.
  - Plan: Split lockfile PID logic by platform (build-tagged implementations), switch specs lock base to `os.TempDir()` (and keep TMPDIR override), and add/adjust tests so `go test ./...` remains cross-platform.

- [ ] RQ-0452 [docs]: Align on-screen key hints and help output with actual bindings (remove misleading hints, document new shortcuts, add guard tests). (ralph_tui/internal/tui/dashboard_view.go, ralph_tui/internal/tui/help_keymap.go, ralph_tui/internal/tui/keymap.go)
  - Evidence: Several screens embed hard-coded "Keys:" lines that can drift from `keymap.go` (e.g., Dashboard advertises fixup blocked). This breaks discoverability and causes user confusion.
  - Plan: Centralize key-hint rendering (or derive from `keyMap`), update view strings, and add tests that assert advertised keys exist and are handled.

## Blocked

## Parking Lot
