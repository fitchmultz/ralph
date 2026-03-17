# Ralph Roadmap

Last updated: 2026-03-17

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Split the remaining oversized Rust runtime and operational helpers

Why first:
- This is the most meaningful remaining roadmap work because it touches reliability-sensitive production paths instead of pure maintenance-only test structure.
- Webhook, queue-maintenance, processor execution, filesystem helpers, and execution-history modules still mix orchestration, persistence, retries, and formatting in ways that increase failure risk during user-facing workflows.
- Doing this pass first improves the production seams that future feature work and maintenance will rely on.

Scope:
- Decompose the remaining oversized operational helpers (`crates/ralph/src/webhook/worker.rs`, `crates/ralph/src/webhook/diagnostics.rs`, `crates/ralph/src/queue/prune.rs`, `crates/ralph/src/queue/hierarchy.rs`, `crates/ralph/src/plugins/processor_executor.rs`, `crates/ralph/src/runutil/execution/orchestration.rs`, `crates/ralph/src/fsutil.rs`, `crates/ralph/src/execution_history.rs`, and adjacent support modules) into focused companions.
- Preserve webhook reload/retry contracts, queue safety behavior, and managed-subprocess invariants while extracting helpers from the root modules.
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

### 4. Split the remaining oversized Rust test and fixture hubs

Why fourth:
- This is the most maintenance-oriented remaining work and should stay behind production-facing refactors.
- Large files such as `task_lifecycle_test.rs`, `run_parallel_test.rs`, `prompt_cli_test.rs`, and queue-operation suites remain clear follow-on churn hotspots, but they are less meaningful than improving runtime behavior.
- Sequencing test-hub splits last minimizes duplicate test moves while contracts are still settling.

Scope:
- Break remaining oversized Rust test and fixture files into thin suite roots plus behavior-grouped companions/directories.
- Preserve current coverage names, helper contracts, and `make agent-ci` / `make ci` verification behavior.
- Keep shared test support centralized only where duplication is real; otherwise prefer adjacent grouped helpers.

### 5. Remove the remaining measured Rust test-serialization bottlenecks

Why fifth:
- Headless profiling still provides the right evidence loop for this work, but the latest targeted reruns no longer show an adjacent parallel safety suite that clearly justifies another refactor pass.
- The cached `.ralph` fixture and faster `agent-ci` routing have already removed the most obvious setup tax from the recently touched parallel suites.
- Keep this lane evidence-driven and dormant until a future timing pass re-ranks a suite high enough to be worth the churn.

Scope:
- Keep target-specific nextest profiles (`run_parallel_test`, `parallel_direct_push_test`, `parallel_done_json_safety_test`, and `doctor_contract_test` as needed) plus `target/profiling/nextest*.jsonl` artifacts as the ranking source of truth.
- Re-profile a candidate suite immediately before editing it and again immediately after any focused fixture/lock cutover instead of repeating whole-workspace sweeps.
- Narrow global-environment locks to only the tests that mutate PATH or shared process env; tests using explicit runner binary overrides should not serialize on `env_lock()`.
- Keep real `ralph init` contract tests explicit while continuing to route pure fixture setup through cached seeded scaffolding.
- Do not reopen `run_parallel_test`, `parallel_direct_push_test`, `parallel_done_json_safety_test`, or `doctor_contract_test` unless fresh measurements show they have re-emerged as worthwhile optimization targets.

### 6. Profile and tighten the headless macOS ship gate internals

Why sixth:
- After docs/community routing and Rust-side test speedups, `macos-ci` is the most expensive remaining default gate for app-surface changes.
- The current `RALPH_XCODE_JOBS` cap and shared Xcode lock are intentionally conservative, but they should now be justified by measured headless build/test timings rather than habit.
- This work should follow the Rust-side cleanup so macOS measurements are easier to isolate.

Scope:
- Measure `macos-build`, `macos-test`, and `macos-test-contracts` separately with current defaults and capped/unbounded Xcode parallelism.
- Preserve headless defaults and keep interactive UI automation outside `macos-ci`.
- Only relax Xcode serialization or default caps when the measured regression risk is understood and covered by contract tests.

### 7. Add durable local timing regression visibility for CI/test loops

Why seventh:
- One-off profiling is useful, but the repo still needs a repeatable way to spot when `make agent-ci`, `make test`, or specific nextest targets get slower again.
- Capturing local timing evidence keeps future optimization work honest without introducing remote CI dependencies.
- This belongs after the main bottlenecks are reduced so new thresholds reflect the improved baseline.

Scope:
- Add a documented local profiling entrypoint that records `make agent-ci`, nextest, and doctest timings under `target/profiling/`.
- Keep the profiling path headless and opt-in; it should not slow the default CI gate.
- Prefer machine-readable summaries that make the slowest targets and trend deltas obvious.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Preserve the recently hardened bundle/versioning/plugin/tutorial contracts while refactoring adjacent modules.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed macOS Settings/workspace-routing or the completed git/init/app split cutovers unless a new regression appears.
