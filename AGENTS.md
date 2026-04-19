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
| GitHub Actions (single allowed workflow) | `.github/workflows/cursor-finish-line-ready.yml` — not canonical CI; see below |
| Core source | `crates/ralph/src/` |
| Tests | `crates/ralph/tests/` |
| macOS app | `apps/RalphMac/` |

---

## User Preferences

- **CI-first**: Run `make agent-ci` before claiming completion (current uncommitted local diff routes to `ci-docs`, `ci-fast`, `ci`, or `macos-ci`; clean tree is a no-op)
- **Release gate**: Run `make release-gate` before release tagging/public launch windows
- **Public-readiness gate**: Use `make pre-public-check` before making broad visibility changes
- **Resource controls**: Prefer `RALPH_CI_JOBS` / `RALPH_XCODE_JOBS` caps on shared workstations
- **Minimal APIs**: Default to private; prefer `pub(crate)` over `pub`
- **Small files**: Target <500 LOC; hard limit at 1,000 LOC (must split)
- **Explicit over implicit**: Prefer explicit, minimal usage patterns
- **Verify before done**: Test coverage required for all new/changed behavior
- **Roadmap quality**: Use chunky, dependency-aware roadmap items; combine adjacent evidence/cleanup/tuning work instead of splitting follow-ups into tiny tasks
- **CI source of truth**: Local `make agent-ci` / `make release-gate` is canonical; do not treat GitHub Actions as a substitute gate

### GitHub Actions (explicit repo exception)

Global Cursor agent rules for this workspace class default to **no GitHub Actions**. **Maintainers have granted a narrow exception for the current minimal workflow only** so agents should not delete it or “fix” the repo by removing `.github/workflows/`.

- **Allowed**: `.github/workflows/cursor-finish-line-ready.yml` — triggers on completed `check_run`, uses `actions/github-script@v7` with `checks: write`, `issues: write`, and read-only `contents` / `pull-requests` permissions, polls until three named **Cursor Automation** checks succeed on the PR head SHA, mirrors that readiness onto a dedicated PR-head `Cursor Finish Line Ready` check run, and applies/removes the `cursor-finish-line-ready` PR label for downstream PR Finish Line sequencing. It is **not** build or test CI; the workflow file’s own header states it is demo automation sequencing only.
- **Not allowed without a new maintainer decision**: additional workflows, matrices, caching layers, release automation, or moving `make agent-ci` / `make release-gate` logic into Actions.

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
- Repo-local execution settings (for example `agent.ci_gate`, runner binary overrides, plugin runner selection, and `plugins.*`) are gated by local `.ralph/trust.jsonc`, not shared config.
- Trust file shape: `{"allow_project_commands": true, "trusted_at": "<RFC3339 optional>"}`.
- Missing trust file means the repo is untrusted.
- Untrusted repos ignore project-scope plugins under `.ralph/plugins`; only trusted repos may execute project-local plugins.
- Plugin manifest executables must stay plugin-dir-relative; absolute and escaping paths are invalid.

### Queue Load/Validate Semantics
- `queue::load_and_validate_queues` is read-only: it may tolerate JSON syntax repair in memory, but it must never rewrite queue/done files.
- Non-UTC RFC3339 timestamps and missing terminal `completed_at` are validation failures for pure read/load paths; they are not silently normalized on read.
- Conservative timestamp normalization/backfill lives behind undo-backed apply flows such as `queue::apply_queue_repair_with_undo`, `queue::apply_queue_maintenance_repair_with_undo`, or `ralph queue repair`.

### Repo Target Resolution
- Repo/file targeting is always derived from process CWD (`find_repo_root(current_dir)`).
- `RALPH_REPO_ROOT_OVERRIDE`, `RALPH_QUEUE_PATH_OVERRIDE`, and `RALPH_DONE_PATH_OVERRIDE` are unsupported.

### Repo Execution Trust
- Repo-local execution settings are trust-gated through local-only `.ralph/trust.jsonc`.
- Missing trust file means repo config may not define `agent.ci_gate`, runner binary overrides, plugin runner selections, or `plugins.*`; move those settings to trusted global config or create the local trust file.
- `.ralph/trust.jsonc` must remain untracked.

### CI Gate Execution
- `agent.ci_gate` is argv-only. Shell-string execution is unsupported; reject shell launchers such as `sh -c`, `cmd /C`, `pwsh -Command`, or `powershell -Command`.

### Managed Subprocesses
- Non-runner operational subprocesses (CI, git/gh, doctor probes, processor hooks, integration sync checks) should flow through `runutil::shell` managed execution with timeout classes, bounded capture, and SIGINT-before-SIGKILL escalation. Do not reintroduce raw `Command::output()` in those paths.
- Managed subprocess and runner wait paths should prefer exit-event waiters plus deadline scheduling over fixed `try_wait` polling loops; if a control slice is unavoidable, keep it centralized and short-lived.
- Use `execute_checked_command` for non-runner subprocesses that must succeed, and classify them with the narrowest timeout bucket (`MetadataProbe`, `AppLaunch`, `MediaPlayback`, etc.) so status handling stays centralized instead of being reimplemented in leaf modules.

### Runtime Module Boundaries
- `runner.rs` is a thin facade only; shared invocation/resume dispatch lives in `runner/invoke.rs`, and external plugin registry/bootstrap lives in `runner/plugin_dispatch.rs`.
- Runner stream handling is split by concern: `execution/stream.rs` wires the sink API, `stream_reader.rs` owns IO loops, `stream_buffer.rs` owns truncation, `stream_events.rs` owns event extraction/correlation, `stream_tool_details.rs` owns compact tool formatting, and `stream_render.rs` owns sink/handler fanout.
- `commands/prompt/mod.rs` is a facade only; template management lives in `prompt/management.rs`, shared preview option types in `prompt/types.rs`, worker preview assembly in `prompt/worker.rs`, scan preview assembly in `prompt/scan.rs`, task-builder preview assembly in `prompt/task_builder.rs`, and prompt tests in `prompt/tests.rs`.
- `commands/prd/mod.rs` is a facade only; markdown parsing lives in `prd/parse.rs`, task generation in `prd/generate.rs`, workflow/load-save orchestration in `prd/workflow.rs`, and PRD tests in `prd/tests.rs`.
- `commands/watch/event_loop/mod.rs` is orchestration-only; immediate notify-event handling lives in `watch/event_loop/events.rs`, timeout/debounce drain checks live in `watch/event_loop/timeout.rs`, and watch-loop tests live in `watch/event_loop/tests.rs`.
- `commands/watch/tasks/mod.rs` is orchestration-only; watch-task creation lives in `watch/tasks/materialize.rs`, reconciliation/deduplication lives in `watch/tasks/reconcile.rs`, queue load/save orchestration lives in `watch/tasks/orchestrator.rs`, and watch task tests live in `watch/tasks/tests.rs`.
- `queue/loader/mod.rs` is a facade only; read/load entrypoints live in `loader/read.rs`, explicit repair flows in `loader/maintenance.rs`, queue-set validation in `loader/validation.rs`, and loader tests under `loader/tests/`.
- `queue/hierarchy.rs` is a facade only; hierarchy indexing/navigation lives in `hierarchy/index.rs`, parent-cycle detection in `hierarchy/cycles.rs`, tree rendering in `hierarchy/render.rs`, and hierarchy tests in `hierarchy/tests.rs`.
- `queue/prune/mod.rs` is a facade only; prune option/report types live in `queue/prune/types.rs`, done-queue prune execution and timestamp ordering live in `queue/prune/core.rs`, and prune regression coverage lives in `queue/prune/tests.rs`.
- `fsutil/mod.rs` is a facade only; tilde expansion lives in `fsutil/paths.rs`, Ralph temp-root/temp-file creation and stale cleanup live in `fsutil/temp.rs`, safeguard dump redaction and raw-dump opt-in flow live in `fsutil/safeguard.rs`, atomic writes and best-effort directory sync live in `fsutil/atomic.rs`, and fsutil regression tests live in `fsutil/tests.rs`.
- `runutil/execution/mod.rs` is a facade only; invocation types/output-capture helpers live in `execution/backend.rs`, continue-session recovery lives in `execution/continue_session.rs`, retry admission lives in `execution/retry_policy.rs`, and runner execution orchestration is a directory-backed facade under `execution/orchestration/{mod.rs, core.rs, failure_paths.rs, tests.rs}`.
- `runutil/retry/mod.rs` is a facade only; policy materialization lives in `retry/policy.rs`, RNG helpers live in `retry/rng.rs`, backoff math + duration formatting live in `retry/backoff.rs`, and retry regression coverage lives in `retry/tests.rs`.
- `runutil/shell/mod.rs` is a facade only; argv validation/building lives in `shell/argv.rs`, shared subprocess types/timeouts/errors live in `shell/types.rs`, managed execution + status enforcement live in `shell/execute.rs`, cancellation-aware retry sleeping lives in `shell/sleep.rs`, capture buffering lives in `shell/capture.rs`, wait/signal escalation lives in `shell/wait.rs`, and shell regression coverage lives in `shell/tests.rs`.
- `migration/config_migrations/mod.rs` is a facade only; config-key detection lives in `config_migrations/detect.rs`, rename/remove helpers live in `config_migrations/keys.rs`, CI gate rewrite lives in `config_migrations/ci_gate.rs`, legacy contract upgrade lives in `config_migrations/legacy.rs`, and config migration tests live in `config_migrations/tests.rs`.
- `migration/file_migrations/mod.rs` is a facade only; generic rename/rollback behavior lives in `file_migrations/rename.rs`, config-reference updates live in `file_migrations/config_refs.rs`, JSON→JSONC wrappers live in `file_migrations/json_to_jsonc.rs`, and file migration tests live in `file_migrations/tests.rs`.
- `template/variables.rs` is a facade only; template-variable data types live in `template/variables/context.rs`, validation lives in `template/variables/validate.rs`, context and git detection live in `template/variables/detect.rs`, substitution lives in `template/variables/substitute.rs`, and regression tests live in `template/variables/tests.rs`.
- `template/loader/mod.rs` is a facade only; template-loader data types live in `template/loader/types.rs`, template resolution + context application live in `template/loader/load.rs`, template listing/existence helpers live in `template/loader/list.rs`, and regression tests live in `template/loader/tests.rs`.
- `agent/resolve/mod.rs` is a facade only; resolved override data lives in `agent/resolve/types.rs`, run/task override parsing lives in `agent/resolve/run.rs`, per-phase override assembly lives in `agent/resolve/phase_overrides.rs`, RepoPrompt fallback bridging lives in `agent/resolve/repoprompt.rs`, and regression tests live in `agent/resolve/tests.rs`.
- `redaction/mod.rs` is a facade only; environment-key detection and sensitive env caching live in `redaction/env.rs`, string-pattern redaction lives in `redaction/patterns.rs`, redacted logger/string helpers live in `redaction/logging.rs`, and regression tests live in `redaction/tests.rs`.
- `eta_calculator.rs` is a facade only; ETA models and confidence helpers live in `eta_calculator/types.rs`, history-backed ETA logic lives in `eta_calculator/calculator.rs`, duration formatting lives in `eta_calculator/format.rs`, and regression tests live in `eta_calculator/tests.rs`.
- `undo.rs` is a facade only; undo snapshot/result data models live in `undo/model.rs`, snapshot storage/list/load helpers live in `undo/storage.rs`, retention pruning lives in `undo/prune.rs`, restore orchestration lives in `undo/restore.rs`, and regression tests live in `undo/tests.rs`.
- `contracts/task.rs` is a facade only; task/task-agent/status data models live in `contracts/task/types.rs`, task priority ordering/parsing lives in `contracts/task/priority.rs`, serde/schema helpers live in `contracts/task/serde_helpers.rs`, and regression tests live in `contracts/task/tests.rs`.
- Directory-backed runtime helper splits should follow the established facade pattern: the root module owns `//!` purpose/responsibilities/scope/usage/invariants docs plus re-exports only, while implementation and regression tests live in adjacent companion files.
- `cli/mod.rs` is a facade only; top-level clap types live in `cli/args.rs`, shared helper/utility entrypoints live in `cli/helpers.rs`, and top-level parse-regression coverage lives in `cli/tests.rs`.
- `cli/machine/mod.rs` is a facade only; clap types live in `cli/machine/args.rs`, shared config/queue helpers live in `cli/machine/common.rs`, top-level routing lives in `cli/machine/handle.rs`, JSON/stdin/stdout helpers live in `cli/machine/io.rs`, queue commands live in `cli/machine/queue.rs`, run commands live in `cli/machine/run.rs`, and task commands/tests live in `cli/machine/task.rs`.
- `cli/prompt/mod.rs` is a facade only; clap types live in `cli/prompt/args.rs` and routing/default resolution lives in `cli/prompt/handle.rs`.
- `cli/queue/import/mod.rs` is a facade only; import input lives in `queue/import/input.rs`, format parsing in `queue/import/parse.rs`, task cleanup in `queue/import/normalize.rs`, duplicate-policy merging in `queue/import/merge.rs`, and operator summaries in `queue/import/report.rs`.
- `cli/queue/issue/mod.rs` is a facade only; clap types live in `queue/issue/args.rs`, shared publish/filter helpers live in `queue/issue/common.rs`, command orchestration lives in `queue/issue/handle.rs`, stdout/prompt rendering lives in `queue/issue/output.rs`, and GitHub issue create/update workflows live in `queue/issue/publish.rs`.
- `cli/task/batch.rs` is a router only; shared batch context, selection, dry-run rendering, status handling, and mutations live under `cli/task/batch/`.
- `cli/task/mod.rs` should stay a thin re-export surface; task command dispatch lives in `cli/task/handle.rs`.
- `commands/task/decompose/mod.rs` is a facade only; planner execution/prompt parsing live in `task/decompose/planning.rs`, source/attach resolution lives in `task/decompose/resolve.rs`, shared materialization helpers live in `task/decompose/support.rs`, tree normalization lives in `task/decompose/tree.rs`, queue-write orchestration lives in `task/decompose/write.rs`, shared data models live in `task/decompose/types.rs`, and decomposition tests live in `task/decompose/tests.rs`.
- `commands/task/update/mod.rs` is an orchestration facade only; dry-run preview printing lives in `task/update/preview.rs`, change reporting lives in `task/update/reporting.rs`, prompt/runner execution lives in `task/update/runner.rs`, queue backup/validation helpers live in `task/update/state.rs`, and update-specific tests live in `task/update/tests.rs`.
- `commands/context/mod.rs` is a facade only; project detection lives in `context/detect.rs`, markdown parsing in `context/markdown.rs`, template rendering in `context/render.rs`, command workflows in `context/workflow.rs`, validation in `context/validate.rs`, shared data types in `context/types.rs`, interactive wizard helpers live under `context/wizard/` (`mod.rs` facade, prompt abstractions in `wizard/prompt.rs`, scripted test prompter in `wizard/scripted.rs`, init/update flows in `wizard/init.rs` and `wizard/update.rs`, wizard data models in `wizard/types.rs`, tests in `wizard/tests.rs`), and broader context tests remain under `context/tests/`.
- `commands/plugin/mod.rs` is a facade only; shared scope/help helpers live in `plugin/common.rs`, list rendering in `plugin/list.rs`, validation in `plugin/validate.rs`, install/uninstall workflows in `plugin/install.rs`, scaffold generation in `plugin/init.rs`, and stub script templates in `plugin/templates.rs`.
- `commands/run/phases/phase3/mod.rs` is a facade only; review prompt assembly lives in `phase3/prompt.rs`, non-final CI flow in `phase3/non_final.rs`, final-iteration integration/finalization in `phase3/finalization.rs`, and completion checks in `phase3/completion.rs`.
- `commands/run/run_loop/mod.rs` is orchestration-only; session recovery lives in `run_loop/session.rs` and queue waiting/unblocked notifications live in `run_loop/wait.rs`.
- `commands/run/parallel/integration/mod.rs` is a facade only; keep configuration/data types in `integration/types.rs`, blocked-marker/handoff persistence in `integration/persistence.rs`, compliance checks in `integration/compliance.rs`, prompt construction in `integration/prompt.rs`, retry/control flow in `integration/driver.rs`, and regression coverage in `integration/tests.rs`.
- `commands/run/parallel/orchestration/mod.rs` is orchestration-only; worker exit/state bookkeeping helpers live in `orchestration/events.rs`.
- `commands/run/parallel/worker.rs` is a facade only; keep selection in `worker_selection.rs`, command construction in `worker_command.rs`, and child lifecycle in `worker_process.rs`.
- `commands/run/parallel/sync/tests/mod.rs` is a thin test hub only; runtime sync cases live in `sync/tests/runtime.rs`, bookkeeping/custom queue-done cases live in `sync/tests/bookkeeping.rs`, and gitignored allowlist coverage lives in `sync/tests/gitignored.rs`.
- `commands/run/tests/mod.rs` is a thin test hub only; shared run-test builders live in `tests/builders.rs`, log capture in `tests/logger.rs`, low-level lock/PID helpers in `tests/support.rs`, common task fixtures in `tests/task_fixtures.rs`, queue-lock regression coverage in `tests/queue_lock.rs`, and phase-settings matrix cases under `tests/phase_settings_matrix/` (`mod.rs` hub with precedence/defaulting/execution-mode/validation/integration companions).
- Parallel worker completion is event-driven: monitor threads own `Child::wait()` and report exits through `ParallelCleanupGuard` channels; do not reintroduce coordinator-side child polling loops.
- `commands/run/supervision/ci.rs` owns CI execution/retry/escalation only; pattern detection is in `ci_patterns.rs` and formatting is in `ci_format.rs`.
- `runner/execution/process/mod.rs` is orchestration-only; process cleanup lives in `process/cleanup.rs` and wait/kill escalation lives in `process/wait.rs`.
- `webhook/diagnostics.rs` is a facade only; runtime counters/snapshots live in `diagnostics/metrics.rs`, failure-store persistence/redaction lives in `diagnostics/failure_store.rs`, replay selection/execution lives in `diagnostics/replay.rs`, and test-only wrappers live in `diagnostics/tests.rs`.
- `reports/stats/mod.rs` is a facade only; report data types live in `reports/stats/model.rs`, task-set shaping/time tracking in `reports/stats/summary.rs`, breakdowns in `reports/stats/breakdowns.rs`, ETA integration in `reports/stats/eta.rs`, text rendering in `reports/stats/render.rs`, and stats tests under `reports/stats/tests/`.
- `reports/aging/mod.rs` is a facade only; thresholds live in `reports/aging/thresholds.rs`, anchor/bucket logic in `reports/aging/compute.rs`, output models in `reports/aging/model.rs`, report construction in `reports/aging/report.rs`, rendering in `reports/aging/render.rs`, and aging tests under `reports/aging/tests/`.
- Large Rust scenario suites should keep a thin root hub and move behavior-grouped cases into adjacent subdirectories (for example `runtime_tests/`, `ci_tests/`, `worker_tests/`, `config_test/`, `doctor_contract_test/`, `queue_stats_history_test/`) so failure locality stays sharp.
- Large queue/context/Makefile contract suites should follow the same root-hub rule: `queue/validation_runtime_tests.rs`, `tests/context_cmd_test.rs`, and `tests/makefile_ci_contract_test.rs` are thin hubs that delegate to adjacent grouped modules/directories.

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
- Worker post-run restore must purge generated runtime artifacts under `.ralph/cache/{plans,phase2_final,parallel}` plus `.ralph/{logs,cache/session.json,cache/migrations.jsonc}` before deciding repo dirtiness.

### Parallel Worker Shutdown
- Parallel worker subprocesses should be terminated gracefully first (`SIGINT`) to let worker-side cleanup run before hard kill escalation.
- Worker subprocesses should run in isolated process groups (`setpgid`) to avoid signaling the coordinator group.

### Continue Session Recovery
- Run/session recovery surfaces should narrate one of three operator-visible states: `resuming_same_session`, `falling_back_to_fresh_invocation`, or `refusing_to_resume`.
- Sequential run decisions are centralized in `session::resolve_run_session_decision`; use preview mode for read-only machine/app surfaces and execute mode for real run entrypoints.
- `ralph run one`, `ralph run loop`, `ralph run resume`, machine run events, and RalphMac Run Control should stay semantically aligned on those resume decisions.
- Continue paths should prefer same-session resume, but fall back to a fresh invocation when no session ID is available.
- Known-invalid resume fallback classification is centralized in `runutil::execution::should_fallback_to_fresh_continue`; do not fork runner-specific resume heuristics across execution paths.
- Safe fresh fallback applies to Pi session-file lookup failures (`no session found`, missing session dir/file), Gemini invalid session identifiers, Claude invalid resume IDs / invalid UUID cases, and Opencode session validation failures.
- Unknown resume failures must hard-fail rather than silently rerun fresh.

### Operator Blocking State Contract
- `contracts/blocking.rs` is the canonical operator-facing stalled/waiting model shared across CLI, machine NDJSON, queue runnability summaries, and RalphMac Run Control.
- Use `BlockingState` for coarse "why is Ralph not making progress right now?" narration; keep per-task blocker detail in queue runnability reasons.
- `blocked_state_changed` / `blocked_state_cleared` machine events, `MachineRunSummaryDocument.blocking`, `MachineDoctorReportDocument.blocking`, and `runnability.summary.blocking` must stay semantically aligned.
- `DoctorReport.blocking` is the doctor-side source of truth for human CLI, `ralph doctor --format json`, and `ralph machine doctor report` diagnosis.
- Resume-refusal and invalid-runner-session stalls should surface through `ResumeDecision::blocking_state()` instead of bespoke app/CLI wording.

### Recovery Tooling Continuations
- `ralph task mutate`, `ralph task decompose`, `ralph queue validate`, `ralph queue repair`, and `ralph undo` are normal continuation tools, not emergency-only escape hatches.
- Recovery surfaces should narrate operator state with the canonical `BlockingState` vocabulary (`waiting`, `blocked`, `stalled`) whenever Ralph needs operator intervention.
- Read-only recovery flows must provide explicit next-step guidance; do not rely on warnings or logs alone.
- Any recovery flow that rewrites queue/done must create an undo checkpoint first. `queue repair` is not exempt.
- Machine/app integrations must use versioned machine recovery documents and continuation summaries rather than parsing human CLI output.
- `ralph task mutate --format json` and `ralph task decompose --format json` must be emitted from the same shared document builders used by `ralph machine task ...`; do not maintain parallel JSON envelopes.
- Recovery documents that expose top-level `blocking` must keep it semantically identical to `continuation.blocking`.
- Prefer preserving partial value with safe normalization plus undoable writes over forcing manual queue surgery.

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
- Prefer deterministic waits/signals over `sleep` in tests; inject logical time or use condition-style helpers when coordination matters.
- Portable absolute test paths should come from temp-root helpers (`std::env::temp_dir()` / test-support builders), never hardcoded `/tmp` literals.
- Rust integration support should stay split into focused `crates/ralph/tests/test_support_*` modules with `test_support.rs` as a thin re-export surface only.
- macOS test fixtures should use `RalphCoreTestSupport` for temp workspaces, readiness polling, and cleanup assertions; do not add per-file sleep/poll helpers.
- SwiftUI previews that need workspace URLs should derive them from `PreviewWorkspaceSupport`, not hardcoded temp paths.
- Test cleanup must fail loudly: avoid `try?` for fixture setup/teardown in tests unless the assertion explicitly expects cleanup best-effort behavior.
- Agent-started servers and automation tools (`playwright`, agent-browser, Peekaboo, XCTest/UI harnesses, `ralph daemon serve`, opened RalphMac instances) must be shut down before ending the session; verify no lingering processes, windows, or temp automation profiles remain.

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
- Prefer `make release-verify VERSION=<x.y.z>` before any real release; it prepares the exact local release snapshot that `make release` must publish.
- `scripts/release.sh verify <x.y.z>` prepares release metadata, checks, artifacts, and notes locally and records the snapshot under `target/release-verifications/`.
- `scripts/release.sh execute <x.y.z>` is the only remote-publishing release entrypoint and must consume that verified snapshot.
- `scripts/release.sh reconcile <x.y.z>` is the only supported continuation path after a partial remote failure.
- Release publication is split across `scripts/lib/release_verify_pipeline.sh` (local snapshot prep) and `scripts/lib/release_publish_pipeline.sh` (remote phases); keep `scripts/lib/release_pipeline.sh` as the thin facade only.
- crates.io is the final irreversible cutover: execute/reconcile must push `main`, push `v<version>`, and prepare/upload a GitHub draft release before `cargo publish`, then publish the GitHub release immediately after crates.io succeeds.
- Canonical CLI build orchestration lives in `scripts/ralph-cli-bundle.sh` for Makefile/Xcode/release-artifact consumers; prefer extending that entrypoint over adding new direct Cargo build paths.
- `scripts/versioning.sh sync` also refreshes `Cargo.lock`; treat lockfile drift as a release/versioning failure, not incidental noise.
- `scripts/release.sh` is expected to sync `VERSION`, `Cargo.lock`, `crates/ralph/Cargo.toml`, `apps/RalphMac/RalphMac.xcodeproj/project.pbxproj`, and `apps/RalphMac/RalphCore/VersionValidator.swift` together.
- Make targets automatically prefer the rustup-managed toolchain pinned by `rust-toolchain.toml` when available; use the same pinned toolchain explicitly for direct script invocations if your shell resolves an older Homebrew `rustc`.
- `make release-verify` intentionally leaves release metadata dirty in the working tree until `make release` turns that snapshot into the release commit; that drift includes the committed JSON schemas under `schemas/` produced by `make generate` (`schemas/config.schema.json`, `schemas/queue.schema.json`, `schemas/machine.schema.json`), as enumerated in `scripts/lib/release_policy.sh` (`RELEASE_METADATA_PATHS`).
- `scripts/release.sh` records transaction state under `target/release-transactions/v<version>/state.env`; reconcile that same version instead of inventing skip/partial reruns.
- `scripts/build-release-artifacts.sh` owns `target/release-artifacts/`: it clears stale artifacts before packaging, so do not rely on leftover tarballs in that directory.
- Shared Xcode-build lock wait logging is intentionally one-shot per invocation; if a macOS target is blocked, expect a single wait line rather than repeated per-second spam.

### Secrets
Never commit or print secrets. `.env` and `.env.*` are local-only and MUST remain untracked (`.env.example` is the only exception).

### Public-Release Guardrails
- Required fast safety gate in CI: `check-env-safety` target now delegates to `scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean --allow-no-git`, which keeps source snapshots usable while still rejecting local/runtime artifacts (including `target/`, unallowlisted `.ralph/*`, repo-local env files, and local note files).
- Convenience alias: `make check-repo-safety`.
- `scripts/pre-public-check.sh` delegates focused scans through `scripts/lib/public_readiness_scan.sh`, which invokes `scripts/lib/public_readiness_scan.py` with `PUBLIC_SCAN_EXCLUDES`; keep docs honest about that scope.
- `scripts/pre-public-check.sh` supports `--release-context`, enforces the `.ralph` tracked-file allowlist, and blocks tracked runtime/local-only files (including unallowlisted `.ralph/*`, repo-local env files, and local note files).
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
- App bundling must go through `scripts/ralph-cli-bundle.sh`; do not reintroduce standalone Cargo fallback logic inside the Xcode project or ad hoc macOS targets.

### macOS App Window Routing
- Active-window navigation/task commands should flow through focused scene values (`WorkspaceUIActions` / `WorkspaceWindowActions`), not process-wide `NotificationCenter` broadcasts.
- Unfocused surfaces (menu bar, URL open, app lifecycle) should register and target scene actions through `WorkspaceSceneRouter`; queue pending workspace routes until the matching `WorkspaceView` registers.
- `RalphMacApp.swift` is a thin shell only; keep menu commands in `RalphMacCommands.swift`, URL routing in `RalphMacApp+URLRouting.swift`, bootstrap/helpers in `RalphMacApp+Support.swift`, window root composition in `WindowViewContainer.swift`, and UI-test window policy in `WorkspaceWindowAnchor.swift`.
- Settings-window orchestration belongs to `SettingsService.swift` + `ASettingsInfra.swift`; app/menu surfaces should call the service instead of constructing settings windows directly.

### macOS App Surface Decomposition
- `DependencyGraphView.swift` is presentation-only; graph data shaping lives in `RalphCore/GraphPresentation.swift`, layout simulation lives in `RalphCore/GraphLayoutEngine.swift`, and async view state lives in `DependencyGraphViewModel.swift`.
- `TaskListView.swift` and `TaskDetailView.swift` should stay orchestration-only; move reusable subsections into `TaskListSections.swift` / `TaskDetailSections.swift` and transient edit/UI timing state into dedicated `*TransientState` or `*EditorState` owners.
- `RalphModels.swift` is a facade only; CLI spec models, generic JSON values, argument-building helpers, and task-domain models live in dedicated `Ralph*.swift` files.
- `WorkspaceManager.swift` is a facade over focused `WorkspaceManager+...` files for app defaults, lifecycle, restoration, routing, and versioning; avoid re-accumulating those concerns in the root type.

### macOS Test Boundaries
- Large app/UI suites should stay split by behavior area: launch/task-flow, navigation/keyboard, conflict handling, window routing, runner config/control, offline caching, performance, and error-recovery categories each belong in dedicated test files.

### macOS Task Presentation
- Shared task filtering/sorting for list + kanban lives in `WorkspaceTaskPresentation`; prefer `workspace.taskPresentation()` when a render pass needs both flat and grouped task sets.
- Task ordering must remain deterministic for both ascending and descending sorts; always break ties explicitly rather than inverting a boolean comparator.
- Task conflict merge semantics live in `RalphCore/TaskConflictResolution.swift`; keep SwiftUI conflict views focused on selection/presentation and keep recovery tooling in separate files.

### macOS Analytics State
- Analytics UI must consume `AnalyticsDashboardState` per-section load states (`idle/loading/loaded-empty/failed`) rather than inferring failure from `nil` payloads.
- Successful-but-empty analytics responses are presentation-state decisions (`*.isEmptyForAnalyticsPresentation`), not transport failures; preserve stale section data only through the explicit previous-value load states.

### macOS Queue Refresh
- Queue file watcher refreshes and CLI queue JSON decoding should use `WorkspaceQueueSnapshotLoader` so file IO + decode work stays off the main actor and only final publication happens on main.
- External queue refresh reactions should read `Workspace.lastQueueRefreshEvent`; do not rebroadcast queue changes through process-wide notifications or view-local `.onReceive`.
- Live watcher orchestration belongs to `WorkspaceQueueRuntime`; keep `QueueFileWatcher` as the low-level async event source and surface watcher degradation through `QueueWatcherHealth` / `WorkspaceOperationalHealth`, not logs alone.

### macOS Workspace Decomposition
- Keep `Workspace.swift` focused on shared workspace state plus broad orchestration; move dense feature areas into `Workspace+...` files.
- `Workspace` is `@MainActor` only and acts as a facade over domain owners (`WorkspaceIdentityState`, `WorkspaceCommandState`, `WorkspaceTaskState`, `WorkspaceInsightsState`, `WorkspaceDiagnosticsState`, `WorkspaceRunState`); do not reintroduce `@unchecked Sendable`.
- Runner lifecycle/loop/cancel orchestration lives in `WorkspaceRunnerController`; `Workspace+RunnerState.swift` owns published run state and the workspace-facing API surface. Loop continuation must be explicitly scheduled, not sleep-polled.
- Task edit/bulk-create flows live in `Workspace+TaskMutations.swift`.
- Workspace persistence and working-directory path resolution live in `Workspace+Persistence.swift`; workspace recovery UI state lives in `Workspace+ErrorRecovery.swift`.
- Workspace identity persistence is a single `.snapshot` payload per workspace via `WorkspaceStateStore`; persistence failures must surface as `PersistenceIssue` state instead of `try?` drops.
- Window restoration/version-cache/app-default cleanup must also surface `PersistenceIssue` through `WorkspaceManager`; crash-report storage failures surface through `CrashReporter.operationalIssues`.
- Operational visibility is unified through `WorkspaceOperationalHealth`: watcher, persistence, crash-report, and CLI health should publish there so banners/indicators/sheets share one severity model.
- Console stream parsing is incremental: hot-path chunks must flow through `consumeStreamTextChunk(_:)` / `WorkspaceStreamProcessor` rather than reparsing full accumulated output.

### macOS CLI Client Decomposition
- Keep `RalphCLIClient.swift` focused on the core subprocess API (`start`, `runAndCollect`, timeout handling).
- Retry helpers live in `RalphCLIClient+Retry.swift`; recovery classification lives in `RalphCLIClient+Recovery.swift`; health probing lives in `RalphCLIHealthChecker.swift`; process lifecycle ownership lives in `RalphCLIRun.swift`.
