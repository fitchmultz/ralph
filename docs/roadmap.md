# Ralph Roadmap

Last updated: 2026-03-20

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Make Ralph feel like an operator console for nondeterministic agent runs, not a generic agent wrapper

Why first:
- Ralph's biggest product risk is not missing another model integration; it is leaving operators unsure what happened, what Ralph is doing now, and what the safest next action is after a nondeterministic run goes sideways.
- The highest-traffic product surfaces are already the run, recover, inspect, and repair loops: `ralph run one`, `ralph run loop`, `ralph run resume`, queue inspection commands, `ralph undo`, and the app's Queue, Run Control, Quick Actions, and task-detail flows.
- First run, resume, retry, and fresh re-invocation can diverge; Ralph has to narrate state, confidence, and operator choices explicitly instead of acting like a deterministic build tool.

Primary outcome:
- Operators should be able to tell, from the CLI or app, whether Ralph is resuming, rerunning fresh, waiting safely, blocked on a real invariant, or asking for a decision.

Detailed execution plan:

#### 1.1 Make resume state and fallback behavior legible
- Surface whether Ralph is:
  - resuming same-session
  - falling back to a fresh invocation
  - refusing to resume because safety is unclear.
- Ensure parity across:
  - `ralph run one`
  - `ralph run loop`
  - `ralph run resume`
  - app Run Control surfaces.
- Revisit session summaries so they reflect actual continuation strategy rather than only stored session metadata.

#### 1.2 Improve blocked / waiting / stalled-state visibility
- Differentiate:
  - dependency blocking
  - schedule blocking
  - lock contention
  - CI fallout
  - runner/session issues
  - true idle waiting.
- Keep queue/status/aging/burndown/app views consistent so they describe the same state model.

#### 1.3 Turn supervision errors into decision support
- Present detected CI patterns, retry state, and current escalation reason clearly.
- Make it obvious when `git_revert_mode` or runner/session capability is the real cause of the stop.
- Use the same decision language in CLI help, doctor output, and app messaging where practical.

#### 1.4 Normalize recovery tooling into the happy path
- Make `task mutate`, `task decompose`, `queue validate`, `queue repair`, and `undo` feel like normal continuation tools, not emergency escape hatches.
- Preserve partial value wherever safe instead of forcing operators into manual queue surgery.

#### 1.5 Tighten parallel only after serial recovery is boring
- Do not spend major churn on `run parallel` UX until serial run/resume/supervision behavior is calm and legible.
- When parallel work resumes, focus on bookkeeping visibility, stale lock handling, and post-run integration clarity.

Exit criteria for item 1:
- Operators can explain what Ralph is doing now and why, without reading source code.
- Session, waiting, and escalation messages are aligned across CLI and app surfaces.

### 2. Keep maintenance and structural hygiene active, but only when it helps the operator-facing roadmap move faster

Why second:
- Cleanup matters when it makes the run/recover/supervise surfaces safer to change, easier to validate, and easier to explain.
- Ralph already has the baseline architecture it needs; additional hygiene work should now be selective, evidence-driven, and tied to product-facing iteration speed.
- A small maintenance lane keeps test and fixture debt from slowing the UX work above without letting maintenance become the default priority again.

Primary outcome:
- Keep the repo easy to change in the areas the roadmap is actively using, without reopening broad cleanup churn.

Detailed execution plan:

#### 2.1 Split only where it improves failure locality or iteration speed
- Prioritize large suites and fixtures that slow operator-critical work.
- Keep `task_template_commands_test.rs` as the main known candidate, not an automatic next task.

#### 2.2 Keep profiling evidence-driven
- Re-profile before and after focused test work.
- Use target-specific nextest artifacts under `target/profiling/` instead of intuition-driven speed work.

#### 2.3 Keep environment isolation narrow and intentional
- Serialize tests that mutate shared PATH or process-global env.
- Avoid over-serializing tests that already use explicit runner overrides or isolated temp roots.

#### 2.4 Preserve real contract coverage on core operator paths
- Keep real `ralph init`, queue/undo/recovery, and supervision coverage where behavior correctness matters.
- Use cached or synthetic scaffolding only when the contract under test is not runtime fidelity.

#### 2.5 Maintain CLI / machine / app semantic parity
- Whenever operator-facing workflow changes land, verify equivalent semantics across:
  - human CLI
  - machine CLI
  - app integrations.

Exit criteria for item 2:
- Structural cleanup measurably reduces iteration pain on active roadmap paths.
- Maintenance work stays in service of product-facing clarity and reliability.

### 3. Add durable local timing visibility, then use it to tune the headless ship gate with evidence

Why third:
- Operator-facing improvements are easier to sustain when local validation speed regressions are visible instead of discovered by feel.
- Durable measurement keeps maintenance work and macOS gate tuning grounded in evidence instead of vibes.
- `macos-ci` is still expensive enough that any relaxation of serialization or job caps should be justified with data and protected by contract coverage.

Primary outcome:
- Validation performance should be observable locally, reproducible, and trendable before any CI-gate simplification decisions are made.

Detailed execution plan:

#### 3.1 Provide one clear local profiling entrypoint
- Add or document an opt-in command that writes machine-readable timing artifacts under `target/profiling/`.
- Keep the entrypoint headless and local-first.

#### 3.2 Measure the gates that actually influence iteration speed
- Capture timings for:
  - `make agent-ci`
  - targeted nextest suites for run/resume/supervision/undo/operator flows
  - doctests
  - `macos-build`
  - `macos-test`
  - `macos-test-contracts`.

#### 3.3 Separate Rust and Xcode timing stories
- Measure Xcode targets separately from Rust/CLI targets.
- Compare capped vs uncapped `RALPH_XCODE_JOBS` before changing defaults.

#### 3.4 Use evidence before changing serialization or job caps
- Do not relax xcodebuild serialization or default concurrency until profiling plus contract coverage show the tradeoff is safe.

Exit criteria for item 3:
- Timing artifacts exist and are easy to compare locally.
- Gate-tuning discussions can reference data instead of anecdote.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Prefer removing avoidable stochastic-core brittleness before layering on more operator UX around the same brittle flows.
- Keep strict validation at deterministic boundaries and flexible process guidance inside agent reasoning loops.
- Prefer low-churn shared-runtime fixes before large prompt-asset churn, and prompt-asset churn before broad test/doc rewrites.
- Prefer run/resume/recovery clarity and operator control over maintenance-only churn when both are plausible next steps.
- Preserve the recently hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and contracts/task) while refactoring adjacent modules.
- Prefer infrastructure and fixture stabilization before broader feature churn only when that stabilization clearly supports the active operator UX roadmap.
- Do not reopen the completed macOS Settings/workspace-routing or the completed git/init/app split cutovers unless a new regression appears.
