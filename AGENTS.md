# Repository Guidelines (Ralph)

<!-- AGENTS ONLY: This file is exclusively for AI agents, not humans -->

**Keep this file updated** as you learn project patterns. Follow: concise, index-style, no duplication.

> ⚠️ **CRITICAL**: Current date is **February 2026**. Always verify information is up-to-date; never assume 2024 references are current.

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
| Core source | `crates/ralph/src/` |
| Tests | `crates/ralph/tests/` |
| macOS app | `apps/RalphMac/` |

---

## User Preferences

- **CI-first**: Run `make agent-ci` before claiming completion; reserve `make macos-ci` as the ship gate
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
2. `.ralph/config.json`
3. `~/.config/ralph/config.json`
4. Schema defaults

### Repo Target Resolution
- Repo/file targeting is always derived from process CWD (`find_repo_root(current_dir)`).
- `RALPH_REPO_ROOT_OVERRIDE`, `RALPH_QUEUE_PATH_OVERRIDE`, and `RALPH_DONE_PATH_OVERRIDE` are unsupported.

### Phase 1 Follow-up Guardrail
- Follow-up Phase 1 baseline snapshots must exclude mutable `.ralph/**` paths; only baseline dirty paths outside `.ralph/` are immutable.

### Parallel Workspace Runtime Sync
- Worker workspace setup mirrors repo-local `.ralph/` runtime files recursively, but MUST exclude coordinator-only and ephemeral paths: queue/done files and `.ralph/{cache,workspaces,logs,lock}/`.
- Gitignored non-`.ralph` sync remains narrow by design (`.env*` only) to avoid copying heavy build/cache directories.
- Parallel worker post-run bookkeeping restore must always target workspace-local `.ralph/{queue.json,done.json,cache/productivity.json}`.
- Worker post-run supervision should fail fast if those bookkeeping paths remain dirty after restore (never proceed to commit/rebase with queue/done/productivity drift).
- Worker post-run restore must purge generated runtime artifacts under `.ralph/cache/{plans,phase2_final,parallel}` plus `.ralph/{logs,cache/session.json,cache/migrations.json}` before deciding repo dirtiness.

### Parallel Worker Shutdown
- Parallel worker subprocesses should be terminated gracefully first (`SIGINT`) to let worker-side cleanup run before hard kill escalation.
- Worker subprocesses should run in isolated process groups (`setpgid`) to avoid signaling the coordinator group.

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
Never commit or print secrets. `.env` is local-only and MUST remain untracked.
