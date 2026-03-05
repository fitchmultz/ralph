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
| Portfolio reviewer map | `PORTFOLIO.md` |
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

### Queue Load/Validate Auto-Repair
- `queue::load_and_validate_queues` performs conservative maintenance before validation.
- Parseable non-UTC RFC3339 timestamps are normalized to UTC; missing terminal `completed_at` is backfilled.
- Malformed timestamps are never rewritten and still fail validation.
- Maintenance writes back to queue/done during load+validate.

### Repo Target Resolution
- Repo/file targeting is always derived from process CWD (`find_repo_root(current_dir)`).
- `RALPH_REPO_ROOT_OVERRIDE`, `RALPH_QUEUE_PATH_OVERRIDE`, and `RALPH_DONE_PATH_OVERRIDE` are unsupported.

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

### File Size Limits
- Target: <500 LOC
- Soft limit: ~800 LOC (requires justification)
- Hard limit: 1,000 LOC (must split)

### Testing
- Unit tests: `#[cfg(test)]` colocated
- Integration: `crates/ralph/tests/`
- Init tests: Always use `--non-interactive` flag
- CI temp dirs: `${TMPDIR:-/tmp}/ralph-ci.*` (set `RALPH_CI_KEEP_TMP=1` to keep)

### Secrets
Never commit or print secrets. `.env` and `.env.*` are local-only and MUST remain untracked (`.env.example` is the only exception).

### Public-Release Guardrails
- Required fast safety gate in CI: `check-env-safety` target now delegates to `scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean`.
- Convenience alias: `make check-repo-safety`.
- `scripts/pre-public-check.sh` enforces `.ralph` tracked-file allowlist (`README.md`, queue/done/config json/jsonc) and blocks tracked runtime dirs (`cache`, `logs`, `lock`, `workspaces`, `undo`, `webhooks`).

### macOS UI Visual Artifacts
- `make macos-test-ui-artifacts` is the evidence workflow for headed UI runs (enables screenshot capture, exports attachments, writes summary).
- Local iteration should use `make macos-ui-build-for-testing` once, then `make macos-ui-retest` to avoid repeated rebuild/sign prompts from macOS UI automation approval.
- Use `RALPH_UI_ONLY_TESTING=<Target/Class/testMethod>` with `make macos-ui-retest` for focused debugging.
- UI screenshot capture is opt-in only (`RALPH_UI_SCREENSHOTS=1` or `RALPH_UI_SCREENSHOT_MODE`); default `make macos-test-ui` stays lightweight.
- Post-review cleanup is explicit: `make macos-ui-artifacts-clean`.
