# Ralph Roadmap

Last updated: 2026-03-17

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Split the remaining oversized Rust runtime and operational helpers

Why first:
- This is the most meaningful remaining roadmap work because it touches reliability-sensitive production paths instead of pure maintenance-only test structure.
- Runtime orchestration and adjacent operational modules still mix orchestration, persistence, retries, and formatting in ways that increase failure risk during user-facing workflows.
- Doing this pass first improves the production seams that future feature work and maintenance will rely on.

Scope:
- Progress (2026-03-17): `crates/ralph/src/runutil/execution/orchestration.rs` has been split into `runutil/execution/orchestration/{mod.rs, core.rs, tests.rs}` following the established directory-backed facade pattern while preserving the existing `crate::runutil::execution` surface and runner-handling behavior.
- Progress (2026-03-17): `crates/ralph/src/queue/prune.rs` has been split into `queue/prune/{mod.rs, types.rs, core.rs, tests.rs}` following the established facade pattern while preserving prune behavior and caller imports.
- Progress (2026-03-17): `crates/ralph/src/fsutil.rs` has been split into `fsutil/{mod.rs, atomic.rs, paths.rs, safeguard.rs, temp.rs, tests.rs}` following the established facade pattern while preserving all `crate::fsutil::*` imports and behavior.
- Continue the runtime-oriented split pass across the remaining runutil and operational helpers (`crates/ralph/src/runutil/retry.rs`, `crates/ralph/src/runutil/shell/mod.rs`, and adjacent support modules) while keeping adjacent churn localized now that `execution/orchestration`, queue-side prune, and fsutil have been cut over.
- Preserve queue safety behavior, managed-subprocess invariants, and operational reliability contracts while extracting helpers from the root modules.
- Keep shared helpers centralized only where duplication is real; otherwise prefer adjacent behavior-grouped modules.

### 2. Split the remaining oversized Rust command and CLI surfaces

Why second:
- The current Rust file scan still reports 36 files over the 500 LOC target, and only one command/CLI hotspot remains above the line.
- That remaining CLI surface is command-facing, but this work is still primarily maintenance compared with the runtime reliability pass above.
- Clearing it after the runtime pass removes the last oversized command/CLI blocker before deeper shared-module cleanup.

Scope:
- Decompose the remaining oversized command/CLI surface (`crates/ralph/src/cli/queue/tests/issue.rs` and any adjacent queue-test helpers it requires) into a thinner hub plus focused companion files/directories.
- Preserve current CLI/help output and existing queue-command test contracts while moving broad scenario/matrix logic out of the root modules.
- Keep moved test hubs thin and behavior-grouped when the queue split requires neighboring suite-module moves.

### 3. Split the remaining oversized Rust shared-data and foundational modules

Why third:
- Foundational helpers such as migration, template, agent-resolution, redaction, ETA, undo, and task-contract modules are broadly reused, so touching them earlier would amplify churn.
- Once the more meaningful runtime work and the remaining command-facing hotspot are addressed, the dependency picture is clearer and shared-module splits become lower-risk.
- These files remain important debt, but they are still maintenance-oriented compared with the earlier production-surface work.

Scope:
- Decompose the remaining oversized foundational modules (`crates/ralph/src/migration/config_migrations.rs`, `crates/ralph/src/migration/file_migrations.rs`, `crates/ralph/src/template/variables.rs`, `crates/ralph/src/template/loader.rs`, `crates/ralph/src/agent/resolve.rs`, `crates/ralph/src/redaction.rs`, `crates/ralph/src/eta_calculator.rs`, `crates/ralph/src/undo.rs`, `crates/ralph/src/contracts/task.rs`, and adjacent shared helpers) into thinner facades plus focused companions.
- Preserve schema, normalization, redaction, and task-contract behavior exactly while moving parsing/formatting helpers out of the root files.
- Prefer deterministic helper modules and avoid reopening stabilized command/runtime seams unless a true shared abstraction emerges.

### 4. Keep the remaining Rust test work structural and evidence-driven

Why fourth:
- Test and fixture cleanup still matters, but it should stay behind the production-facing refactors above.
- The recently re-profiled parallel suites no longer justify more optimization churn, so remaining test work should focus on clear structural debt plus measured bottlenecks only.
- Combining structural cleanup with profiling guardrails reduces duplicate suite churn while preserving the option to act quickly when a real hotspot re-emerges.

Scope:
- Break remaining oversized Rust test and fixture files into thin suite roots plus behavior-grouped companions/directories when that structural cleanup is still worthwhile on its own.
- Keep shared test support centralized only where duplication is real; otherwise prefer adjacent grouped helpers.
- Use target-specific nextest profiles and `target/profiling/nextest*.jsonl` artifacts as the source of truth before reopening any test-speed pass.
- Re-profile a candidate suite immediately before editing it and again immediately after any focused fixture/lock cutover instead of repeating whole-workspace sweeps.
- Narrow global-environment locks to only the tests that mutate PATH or shared process env; tests using explicit runner binary overrides should not serialize on `env_lock()`.
- Keep real `ralph init` contract tests explicit while continuing to route pure fixture setup through cached seeded scaffolding.
- Do not reopen `run_parallel_test`, `parallel_direct_push_test`, `parallel_done_json_safety_test`, or `doctor_contract_test` unless fresh measurements show they have re-emerged as worthwhile optimization targets.

### 5. Add durable local timing visibility, then use it to tighten the headless macOS ship gate

Why fifth:
- One-off profiling is useful, but the repo still needs a repeatable way to spot when `make agent-ci`, `make test`, or specific nextest/macOS targets get slower again.
- Adding that measurement path first makes later macOS gate tuning easier to justify, repeat, and roll back if needed.
- After the Rust-side cleanup above, `macos-ci` remains the most expensive default gate for app-surface changes and should be tuned only with durable evidence.

Scope:
- Add a documented local profiling entrypoint that records `make agent-ci`, nextest, doctest, and macOS gate timings under `target/profiling/`.
- Keep the profiling path headless and opt-in; it should not slow the default CI gate.
- Prefer machine-readable summaries that make the slowest targets and trend deltas obvious.
- Measure `macos-build`, `macos-test`, and `macos-test-contracts` separately with current defaults and capped/unbounded Xcode parallelism.
- Preserve headless defaults and keep interactive UI automation outside `macos-ci`.
- Only relax Xcode serialization or default caps when the measured regression risk is understood and covered by contract tests.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Preserve the recently hardened bundle/versioning/plugin/tutorial contracts while refactoring adjacent modules.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed macOS Settings/workspace-routing or the completed git/init/app split cutovers unless a new regression appears.
