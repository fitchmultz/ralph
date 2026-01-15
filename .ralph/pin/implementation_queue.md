# Implementation Queue

## Queue
- [ ] RQ-0457 [code]: Make loop finalize completion detection item-ID based (avoid false completion when new unchecked items are inserted). (ralph_tui/internal/loop/loop.go, ralph_tui/internal/loop/queue.go, ralph_tui/internal/loop/loop_test.go)
  - Evidence: `Runner.finalizeIteration()` currently decides `completed` via `firstAfter == nil || firstAfter.ID != itemID`. If a runner adds a new unchecked item at the top of the queue (or tag filters shift what's "first"), this can misclassify the current item as "completed" even if it remains unchecked elsewhere, which risks committing/validating the wrong thing.
  - Plan: Introduce an explicit "item completion" check by scanning queue+done for `itemID` and verifying either (a) it moved to Done, or (b) it is checked and no longer eligible as the active unchecked item; then update finalize logic + add tests covering the "new item inserted above" case.

- [ ] RQ-0458 [code]: Fix git helper semantics (BranchExists error handling + AheadCount parsing) and unblock fixup reliability. (ralph_tui/internal/loop/git.go, ralph_tui/internal/loop/fixup.go, ralph_tui/internal/loop/git_test.go)
  - Evidence: `BranchExists()` checks `err.(*exec.ExitError)` but `gitRun()` wraps failures in `*loop.GitCommandError`, so a missing branch can incorrectly return an error instead of `(false, nil)`. This affects fixup (`validateWipBranchInWorktree` calls `BranchExists` and can abort the whole fixup run). Also `AheadCount()` uses `fmt.Sscanf` without checking the scan error, so invalid output can silently report `0`.
  - Plan: (1) Update `BranchExists` to use `errors.As` to detect the underlying `*exec.ExitError` inside `*GitCommandError` and treat "verify failed" as "branch does not exist", (2) harden `AheadCount` parsing to return an error if the count can't be parsed, and (3) add focused unit tests for both behaviors.

- [ ] RQ-0459 [code]: Unify CLI `--only-tag` parsing with config/tag rules (commas+spaces; validate unknown tags; consistent loop selection). (ralph_tui/cmd/ralph/main.go, ralph_tui/internal/pin/pin.go, ralph_tui/cmd/ralph/main_test.go)
  - Evidence: `splitTagsCLI()` splits only on commas, but `config.Validate()` accepts whitespace-separated tags via `pin.ParseTagList`. A config like `loop.only_tags = "ui code"` can validate, yet CLI parsing would pass `[]string{"ui code"}` into the loop and effectively match nothing. Unknown tags supplied via CLI aren't validated early, leading to confusing "no items found" behavior.
  - Plan: Replace `splitTagsCLI` usage with `pin.ParseTagList` + validation of `Unknown` tags (error fast), normalize tag values consistently, and add CLI unit tests for comma/space/bracket inputs (e.g., `ui, [code] docs`).

- [ ] RQ-0450 [code]: Eliminate DRY violations in runner invocation (specs vs loop); validate opencode args and ensure streaming behavior is consistent across commands. (ralph_tui/internal/loop/runner.go, ralph_tui/internal/specs/specs.go, ralph_tui/internal/runnerargs/effort.go)
  - Evidence: Codex/opencode command construction is duplicated between `loop.RunnerInvoker.RunPrompt` and `specs.runRunner`, increasing the risk of argument drift and inconsistent behavior (including streaming semantics).
  - Plan: Extract shared runner invocation utilities, align opencode argument conventions in both paths, and add hermetic tests that assert arguments and streaming output behavior.

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
