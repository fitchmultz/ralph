# Ralph Roadmap

Last updated: 2026-03-19

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Keep the remaining Rust test work structural and evidence-driven

Why first:
- The remaining production-facing foundational split work is complete, so the next highest-leverage structural debt sits in oversized test and fixture surfaces.
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

### 2. Add durable local timing visibility, then use it to tighten the headless macOS ship gate

Why second:
- One-off profiling is useful, but the repo still needs a repeatable way to spot when `make agent-ci`, `make test`, or specific nextest/macOS targets get slower again.
- Adding that measurement path first makes later macOS gate tuning easier to justify, repeat, and roll back if needed.
- After the completed foundational split work, `macos-ci` remains the most expensive default gate for app-surface changes and should be tuned only with durable evidence.

Scope:
- Add a documented local profiling entrypoint that records `make agent-ci`, nextest, doctest, and macOS gate timings under `target/profiling/`.
- Keep the profiling path headless and opt-in; it should not slow the default CI gate.
- Prefer machine-readable summaries that make the slowest targets and trend deltas obvious.
- Measure `macos-build`, `macos-test`, and `macos-test-contracts` separately with current defaults and capped/unbounded Xcode parallelism.
- Preserve headless defaults and keep interactive UI automation outside `macos-ci`.
- Only relax Xcode serialization or default caps when the measured regression risk is understood and covered by contract tests.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Preserve the recently hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and contracts/task) while refactoring adjacent modules.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed macOS Settings/workspace-routing or the completed git/init/app split cutovers unless a new regression appears.
