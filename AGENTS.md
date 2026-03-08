# Repository Guidelines (Ralph)

<!-- AGENTS ONLY: This file is exclusively for AI agents, not humans -->

**Keep this file updated** as you learn project patterns. Follow: concise, index-style, no duplication.

> ⚠️ **CRITICAL**: Current date is **March 2026**. Always verify information is up-to-date; never assume 2024 references are current.

---

## Goal

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue.

---

## Where to Find Things

| Topic | Location |
|-------|----------|
| Architecture & design | `docs/index.md` |
| Contributing guide | `CONTRIBUTING.md` |
| Configuration | `docs/configuration.md` |
| CLI reference | `docs/cli.md` |
| Public launch checklist | `docs/guides/public-readiness.md` |
| Local verification guide | `docs/guides/local-smoke-test.md` |
| Public audit automation | `scripts/pre-public-check.sh` |
| Core source | `crates/ralph/src/` |
| Tests | `crates/ralph/tests/` |
| macOS app | `apps/RalphMac/` |

---

## User Preferences

- **CI-first**: Run `make agent-ci` before claiming completion (`ci-fast` for non-app changes, `macos-ci` when app paths change)
- **Full Rust gate**: Run `make ci` before release tagging/public launch windows
- **Public-readiness gate**: Use `make pre-public-check` before making broad visibility changes
- **Resource controls**: Prefer `RALPH_CI_JOBS` / `RALPH_XCODE_JOBS` caps on shared workstations
- **Minimal APIs**: Default to private; prefer `pub(crate)` over `pub`
- **Small files**: Target <500 LOC; hard limit at 1,000 LOC (must split)
- **Explicit over implicit**: Prefer explicit, minimal usage patterns
- **Verify before done**: Test coverage required for all new/changed behavior
- **No remote CI**: Local `make ci` is the gate; avoid GitHub Actions

---

## Non-Obvious Patterns

### Error Handling Strategy
Two-tier approach: `anyhow` for propagation, `thiserror` for domain errors.

| Scenario | Pattern |
|----------|---------|
| Propagating | `anyhow::Result<T>` |
| Quick return | `bail!("msg")` |
| Add context | `.context("...")` |
| Domain errors | `thiserror` enums like `RunnerError` |

### Module Documentation Required
Every source file MUST start with `//!` docs covering:
- Responsibilities (what it handles)
- Non-scope (what it explicitly does NOT handle)
- Invariants/assumptions callers must respect

### Session ID Format
`{task_id}-p{phase}-{timestamp}` (Unix epoch seconds). No `ralph-` prefix. Passed via `--session` flag.

### Configuration Precedence
1. CLI flags
2. `.ralph/config.jsonc`
3. `~/.config/ralph/config.jsonc`
4. Schema defaults

### Repo Execution Trust
- Repo-local execution settings (for example `agent.ci_gate`, runner binary overrides, plugin runner selection, and `plugins.*`) are gated by local `.ralph/trust.jsonc` / `.ralph/trust.json`, not shared config.
- Trust file shape: `{"allow_project_commands": true, "trusted_at": "<RFC3339 optional>"}`.
- Missing trust file means the repo is untrusted.
- Untrusted repos ignore project-scope plugins under `.ralph/plugins`; only trusted repos may execute project-local plugins.
- Plugin manifest executables must stay plugin-dir-relative; absolute and escaping paths are invalid.

### Queue Load/Validate Semantics
- `queue::load_and_validate_queues` is read-only: it may tolerate JSON syntax repair in memory, but it must never rewrite queue/done files.
- Non-UTC RFC3339 timestamps and missing terminal `completed_at` are validation failures for pure read/load paths; they are not silently normalized on read.
- Conservative timestamp normalization/backfill lives behind explicit repair flows such as `queue::repair_and_validate_queues` or `ralph queue repair`.

### Repo Target Resolution
- Repo/file targeting is always derived from process CWD (`find_repo_root(current_dir)`).
- `RALPH_REPO_ROOT_OVERRIDE`, `RALPH_QUEUE_PATH_OVERRIDE`, and `RALPH_DONE_PATH_OVERRIDE` are unsupported.

### Repo Execution Trust
- Repo-local execution settings are trust-gated through local-only `.ralph/trust.jsonc` / `.ralph/trust.json`.
- Missing trust file means repo config may not define `agent.ci_gate`, runner binary overrides, plugin runner selections, or `plugins.*`; move those settings to trusted global config or create the local trust file.
- `.ralph/trust.json*` must remain untracked.

### CI Gate Execution
- `agent.ci_gate` is argv-only. Shell-string execution is unsupported; reject shell launchers such as `sh -c`, `cmd /C`, `pwsh -Command`, or `powershell -Command`.

### Notification Audio
- Windows custom notification sounds are `.wav`-only and play through WinMM; do not reintroduce PowerShell-based playback.

### Phase 1 Follow-up Guardrail
- Follow-up Phase 1 baseline snapshots must exclude mutable `.ralph/**` paths; only baseline dirty paths outside `.ralph/` are immutable.

### Parallel Workspace Runtime Sync
- Worker workspace setup mirrors repo-local `.ralph/` runtime files recursively, excluding only ephemeral paths `.ralph/{cache,workspaces,logs,lock}/`.
- Worker workspace setup MUST seed queue/done from coordinator-resolved paths (including `.jsonc` and custom paths) so migrated-uncommitted and gitignored `.ralph` repos work in parallel mode.
- Gitignored non-`.ralph` sync remains narrow by design (`.env*` only) to avoid copying heavy build/cache directories.
- Parallel worker post-run bookkeeping restore must always target workspace-local `.ralph/{queue.json,queue.jsonc,done.json,done.jsonc,cache/productivity.json}`.
- Worker post-run supervision should fail fast if those bookkeeping paths remain dirty after restore (never proceed to commit/rebase with queue/done/productivity drift).
- Worker post-run restore must purge generated runtime artifacts under `.ralph/cache/{plans,phase2_final,parallel}` plus `.ralph/{logs,cache/session.json,cache/migrations.json}` before deciding repo dirtiness.

### Parallel Worker Shutdown
- Parallel worker subprocesses should be terminated gracefully first (`SIGINT`) to let worker-side cleanup run before hard kill escalation.
- Worker subprocesses should run in isolated process groups (`setpgid`) to avoid signaling the coordinator group.

### Continue Session Recovery
- Continue paths should prefer same-session resume, but fall back to a fresh invocation when no session ID is available.
- Pi resume should fall back to fresh invocation when session file lookup fails (`no session found`, missing session dir/file), since Pi may defer session-file persistence until after first assistant output.
- Resume fallback should also cover known invalid-session resume failures for Gemini (`invalid session identifier`), Claude (`--resume requires a valid session ID` / invalid UUID), and Opencode (`ZodError` sessionID validation), not just Pi file lookup failures.

### Runner Output Edge Cases
- Opencode may emit fatal session validation errors on stderr while still exiting with code `0`; treat this as semantic failure rather than success.
- Gemini `stream-json` assistant messages may arrive as delta chunks (`"delta": true`); final-response parsing must accumulate deltas rather than overwriting with the latest chunk.

### Watch Task Identity
- `watch.version = "2"` is the durable watch-task metadata contract: `watch.file`, `watch.line`, `watch.comment_type`, `watch.content_hash`, `watch.location_key`, and `watch.identity_key`.
- Watch deduplication/reconciliation must never rely on title/notes heuristics or content-only fingerprints.
- Removal reconciliation is scoped to the files processed in the current scan batch; untouched files must not be auto-closed.
- Move/rename policy is intentional cutover behavior: moved comments and renamed files close the old watch task and create a new one; structured legacy watch tasks may upgrade in place only on exact same-file same-line matches.

### Task Mutation Transactions
- `ralph task mutate` is the atomic JSON mutation surface for multi-field and multi-task edits; app-side task editing should not shell out field-by-field.
- App optimistic locking should flow through CLI mutation requests via `expected_updated_at`, not bespoke app-only timestamp checks.
- Atomic mutation requests should update status-derived fields (for example `started_at`) through the same transaction path rather than follow-up best-effort edits.

### Signal Recovery
- Signal-terminated runner invocations should auto-attempt recovery up to `MAX_SIGNAL_RESUMES` (default `5`) before surfacing terminal failure handling.
- Signal recovery should reuse session resume when possible and rerun fresh when no resumable session exists.

### CI Gate Cadence
- In 3-phase workflows, final-iteration Phase 2 should skip CI gate rerun; CI is enforced in Phase 3/post-run supervision.
- CI gate logging should emit explicit start/end timing for long-running commands.

### Prompt Mode Signaling
- Completion checklist rendering injects `RUN_MODE` (`normal` or `parallel-worker`).
- Parallel-worker runs must follow `RUN_MODE=parallel-worker` checklist rules (no `ralph task done`; integration updates workspace queue/done and pushes base branch).

### Parallel Merge Refresh Resilience
- After merge-agent success, update state (`prs`, `pending_merges`) before local branch refresh attempts.
- Local base-branch refresh is best-effort and must not abort the run when `.ralph` bookkeeping files are dirty.
- Startup should prune pending merge jobs whose PR lifecycle is no longer open.
- Retry limits (`merge_retries`) must be enforced for every retryable merge outcome (conflict, runtime failure, and merge-agent spawn failure) in both `as_created` and `after_all` flows.
- `gh pr merge` must run with explicit `--repo` from an isolated cwd to avoid mutating the coordinator working tree.
- Parallel worker post-run git finalization should use rebase-aware push (`push_upstream_with_rebase`) so stale non-fast-forward task branches do not require manual intervention.
- Rebase-aware push must also handle branches with no local upstream but an existing remote branch (set tracking to `origin/<branch>` and avoid failing on pure "behind" states).
- Rebase-aware push must retry fetch+rebase+push multiple times on non-fast-forward races (single retry is insufficient under concurrent branch updates).

### Webhook Delivery Runtime
- Webhook delivery runtime is reloadable, not OnceLock-first-wins: config/mode changes must rebuild the dispatcher when effective queue capacity or worker count changes.
- Standard delivery uses a small worker pool; parallel mode scales queue capacity deterministically from `queue_capacity * max(1, workers * parallel_queue_multiplier)`.
- Retries must be scheduled off the hot worker path (timer/scheduler queue), never by sleeping inside a delivery worker.
- All webhook-facing logs/diagnostics/errors must render destinations through the canonical redaction helper; never log raw query strings, userinfo, or token-bearing paths.
- Persisted webhook failure records may store a redacted `destination` for diagnostics, but never raw webhook secrets/headers.

### File Size Limits
- Target: <500 LOC
- Soft limit: ~800 LOC (requires justification)
- Hard limit: 1,000 LOC (must split)

### Testing
- Unit tests: `#[cfg(test)]` colocated
- Integration: `crates/ralph/tests/`
- Init tests: Always use `--non-interactive` flag
- CI temp dirs: `${TMPDIR:-/tmp}/ralph-ci.*` (set `RALPH_CI_KEEP_TMP=1` to keep)

### Task Decompose
- `ralph task decompose` is preview-first; queue mutation requires `--write`.
- The command supports freeform requests, existing-task decomposition, and `--attach-to <TASK_ID>` for freeform subtree attachment.
- `--child-policy fail|append|replace` governs existing child trees; `replace` must refuse when outside tasks still reference the subtree.
- `--with-dependencies` infers sibling-only `depends_on` edges from planner keys/titles.
- Stable machine-readable output is exposed via `ralph task decompose --format json`.

### Release Versioning
- Canonical repo version source is the top-level `VERSION` file.
- Use `./scripts/versioning.sh check` (or `make version-check`) to verify Cargo, Xcode, and app compatibility metadata stay in sync.
- Use `./scripts/versioning.sh sync --version <x.y.z>` (or `make version-sync VERSION=<x.y.z>`) for release bumps; do not hand-edit Cargo/Xcode/version-range files independently.
- Prefer `make release-verify VERSION=<x.y.z>` before any real release; it syncs version metadata, runs the release safety checks, runs the appropriate ship gate, and dry-runs `scripts/release.sh`.
- `make release-verify` intentionally passes `RALPH_RELEASE_ALLOW_EXISTING_TAG=1` to the dry-run release script so re-validating an already-cut version does not fail on the local tag; real `scripts/release.sh` invocations still treat an existing tag as fatal.
- `scripts/versioning.sh sync` also refreshes `Cargo.lock`; treat lockfile drift as a release/versioning failure, not incidental noise.
- `scripts/release.sh` is expected to sync `VERSION`, `Cargo.lock`, `crates/ralph/Cargo.toml`, `apps/RalphMac/RalphMac.xcodeproj/project.pbxproj`, and `apps/RalphMac/RalphCore/VersionValidator.swift` together.
- Make targets automatically prefer the rustup-managed toolchain pinned by `rust-toolchain.toml` when available; use the same pinned toolchain explicitly for direct script invocations if your shell resolves an older Homebrew `rustc`.
- `scripts/release.sh` owns `target/release-artifacts/`: it clears stale artifacts before packaging and on rollback/exit, so do not rely on leftover tarballs in that directory.
- Shared Xcode-build lock wait logging is intentionally one-shot per invocation; if a macOS target is blocked, expect a single wait line rather than repeated per-second spam.

### Secrets
Never commit or print secrets. `.env` and `.env.*` are local-only and MUST remain untracked (`.env.example` is the only exception).

### Public-Release Guardrails
- Required fast safety gate in CI: `check-env-safety` target now delegates to `scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean`.
- Convenience alias: `make check-repo-safety`.
- `scripts/pre-public-check.sh` enforces `.ralph` tracked-file allowlist (`README.md`, queue/done/config json/jsonc) and blocks tracked runtime dirs (`cache`, `logs`, `lock`, `workspaces`, `undo`, `webhooks`).
- `scripts/release.sh` should derive the GitHub repo URL from `git remote origin` and set an explicit GitHub release title (`v<version>`); avoid hardcoded owner-specific release links inside automation.

### macOS UI Visual Artifacts
- `make macos-test-ui-artifacts` is the evidence workflow for headed UI runs (enables screenshot capture, exports attachments, writes summary).
- Local iteration should use `make macos-ui-build-for-testing` once, then `make macos-ui-retest` to avoid repeated rebuild/sign prompts from macOS UI automation approval.
- Use `RALPH_UI_ONLY_TESTING=<Target/Class/testMethod>` with `make macos-ui-retest` for focused debugging.
- Makefile macOS targets serialize `xcodebuild` through `target/tmp/locks/xcodebuild.lock`; prefer the Make targets over ad-hoc concurrent `xcodebuild` invocations.
- UI screenshot capture is opt-in only (`RALPH_UI_SCREENSHOTS=1` or `RALPH_UI_SCREENSHOT_MODE`); default `make macos-test-ui` stays lightweight.
- Post-review cleanup is explicit: `make macos-ui-artifacts-clean`.
- UI-test window geometry is part of the contract: keep normal UI-test launches at one visible workspace window, multiwindow launches at two, and avoid widths below the split-view practical minimum (~950pt) or sidebars/detail panes become cropped/non-hittable.
- UI tests must never write into the production app defaults domain. `RalphAppDefaults` isolates `--uitesting` launches into a dedicated suite and normal launches prune stale `ralph-ui-tests` workspace/restoration keys from `com.mitchfultz.ralph`.
- `ralph app open` should launch with a single `ralph://open?...` URL when workspace context exists. Pre-launching the bundle and then dispatching the URL creates duplicate SwiftUI `WindowGroup` scenes on macOS.
- `make install` on macOS is expected to update both the CLI and `/Applications/RalphMac.app`; otherwise `ralph app open` can keep launching a stale bundle.

### macOS App Window Routing
- Active-window navigation/task commands should flow through focused scene values (`WorkspaceUIActions` / `WorkspaceWindowActions`), not process-wide `NotificationCenter` broadcasts.
- Menu-bar initiated workspace/task routing must carry a `WorkspaceRouteRequest` and be filtered by `workspace.id` so non-target windows stay unchanged.

### macOS Task Presentation
- Shared task filtering/sorting for list + kanban lives in `WorkspaceTaskPresentation`; prefer `workspace.taskPresentation()` when a render pass needs both flat and grouped task sets.
- Task ordering must remain deterministic for both ascending and descending sorts; always break ties explicitly rather than inverting a boolean comparator.

### macOS Queue Refresh
- Queue file watcher refreshes and CLI queue JSON decoding should use `WorkspaceQueueSnapshotLoader` so file IO + decode work stays off the main actor and only final publication happens on main.

### macOS Workspace Decomposition
- Keep `Workspace.swift` focused on shared workspace state plus broad orchestration; move dense feature areas into `Workspace+...` files.
- Runner lifecycle/loop/cancel state lives in `Workspace+RunnerState.swift`; task edit/bulk-create flows live in `Workspace+TaskMutations.swift`.
- Workspace persistence and working-directory path resolution live in `Workspace+Persistence.swift`; workspace recovery UI state lives in `Workspace+ErrorRecovery.swift`.

### macOS CLI Client Decomposition
- Keep `RalphCLIClient.swift` focused on the core subprocess API (`start`, `runAndCollect`, timeout handling).
- Retry helpers live in `RalphCLIClient+Retry.swift`; recovery classification lives in `RalphCLIClient+Recovery.swift`; health probing lives in `RalphCLIHealthChecker.swift`; process lifecycle ownership lives in `RalphCLIRun.swift`.
