# Ralph Roadmap

Last updated: 2026-03-20

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Relax brittle AI-workflow assumptions while keeping strict handoff contracts

Why first:
- The clearest avoidable agent failures now come from over-constraining the stochastic middle of the workflow: exact tool/path assumptions, uneven resume fallback behavior, and prompt/test contracts that freeze wording instead of outcomes.
- Fixing these assumptions improves reliability across runners before more operator-facing polish lands.
- This work has a clean low-churn sequence: shared recovery logic first, then prompt assets, then tests/docs.
- Prompt, runner, and supervision behavior are tightly coupled today; if we do the UX work first, we will just polish brittle contracts that should be loosened.

Primary outcome:
- Ralph should preserve strict contracts at deterministic handoff points while allowing agents to succeed through multiple valid reasoning paths, tool combinations, and wording styles.

Detailed execution plan:

#### 1.1 Unify resume/retry fallback policy across all execution paths
Goal:
- A recoverable invalid-session failure should degrade to a fresh invocation consistently, regardless of whether the call came through shared execution orchestration or post-run supervision.

Work:
- Centralize runner-specific “resume is invalid but rerun is safe” classification in one shared helper.
- Use the same helper in:
  - `runutil/execution/continue_session.rs`
  - `commands/run/supervision/continue_session.rs`
  - any adjacent orchestration paths that currently duplicate session-recovery policy.
- Preserve known recoverable cases already handled in supervision for:
  - Pi missing session files
  - Gemini invalid session identifiers
  - Claude invalid UUID / invalid resume ID cases
  - OpenCode semantic resume-validation failures.
- Keep unknown resume failures hard-failing instead of silently falling back.

Validation:
- Expand shared execution tests so the non-supervision path has parity with supervision resume-fallback coverage.
- Keep runner-specific fixtures explicit so future model/CLI behavior changes can be added without another policy fork.

Why this comes first:
- It directly reduces avoidable run aborts.
- It touches fewer files than prompt/test rewrites.
- It creates a stable runtime substrate for the prompt loosenings below.

#### 1.2 Replace hard-coded prompt assumptions with capability-aware guidance
Goal:
- Prompts should prefer useful paths without assuming one exact environment, one exact home-directory file, or one exact tool inventory.

Work:
- Remove generic hard-coded references to `~/.codex/AGENTS.md` from default prompts when instruction-file injection already exists.
- Reframe prompt context ordering so injected/configured instruction files are authoritative, repo-local docs are advisory, and runner-specific home paths are examples only when truly needed.
- Replace unconditional “use agent swarms / parallel agents / sub-agents aggressively” instructions with capability-conditional language.
- Audit prompt references to specific tools or wrappers that are not guaranteed to exist at runtime.
- Where a preferred tool exists, document it as “prefer when available” rather than “must exist”.

Primary files:
- `crates/ralph/assets/prompts/worker.md`
- `crates/ralph/assets/prompts/worker_phase1.md`
- `crates/ralph/assets/prompts/worker_phase2_handoff.md`
- `crates/ralph/assets/prompts/task_builder.md`
- `crates/ralph/assets/prompts/task_updater.md`
- `crates/ralph/assets/prompts/scan_*.md`
- `crates/ralph/src/prompts_internal/util.rs`

Validation:
- Prompt rendering should still preserve required placeholders and queue/config interpolation.
- Missing optional tools/files should no longer imply failure in prompt guidance unless a real downstream invariant requires them.

#### 1.3 Narrow “must” language to real machine-enforced boundaries
Goal:
- Keep mandatory language only where Ralph or downstream systems actually depend on it.

Keep strict:
- Queue/task schema validity
- Required prompt placeholders
- Plan cache file existence when Phase 2 depends on it
- Phase 2 final-response cache as Phase 3 input when available
- CI gate pass conditions
- Task terminal state requirements in completion paths
- Safety boundaries around queue/done mutation and git publish behavior.

Relax where appropriate:
- Exact headings in freeform handoff text
- Exact prose around blockers and summaries
- Exact internal planning sequence when the final artifact is what matters
- Exact wording for confirmations when no parser consumes that wording.

Work:
- Review every high-salience prompt section that uses “MUST”, “Do NOT”, “exactly”, or “required”.
- Reclassify each instruction as one of:
  - deterministic boundary
  - preferred guidance
  - optional tactic.
- Rewrite prompt text so operators and override authors can tell which is which.

Validation:
- Any remaining hard requirement should map to an actual parser, validator, or runtime invariant.
- If the system cannot detect violation of a prompt instruction, it should usually not be expressed as a brittle exact-format contract.

#### 1.4 Decouple prompt tests from exact prose
Goal:
- Prompt tests should protect behaviorally important contracts, not freeze cosmetic wording.

Work:
- Rewrite prompt tests to focus on semantic invariants such as:
  - required placeholders are rendered
  - unresolved placeholders are absent
  - task IDs / plan paths / run modes are inserted
  - deterministic safety instructions remain present
  - prompt mode selection and wrapper composition remain correct.
- Reduce or remove tests that assert exact headings or exact phrase fragments unless production code parses them.
- Keep a small set of marker tests only for truly intentional public-doc/help surfaces where exact wording is the contract.

Primary files:
- `crates/ralph/tests/promptflow_test.rs`
- `crates/ralph/tests/prompt_cmd_test/worker.rs`
- `crates/ralph/src/prompts_internal/tests/worker.rs`
- `crates/ralph/src/prompts_internal/tests/review.rs`
- `crates/ralph/src/prompts_internal/tests/scan.rs`
- `crates/ralph/src/prompts_internal/tests/registry.rs`

Validation:
- Prompt changes should stop causing broad CI churn when only wording changes.
- Tests should still fail when required artifacts, placeholders, or boundary instructions disappear.

#### 1.5 Align docs with the relaxed-core / strict-edge model
Goal:
- Docs should stop teaching rigid internal scripts unless Ralph truly depends on them.

Work:
- Update docs to separate:
  - mandatory runtime contracts
  - recommended operator workflows
  - illustrative examples.
- Reword any docs that imply one canonical plan markdown structure, one canonical handoff section order, or one canonical tool path when that is only an example.
- Make prompt override docs explicit about preserving placeholders and invariants, while allowing wording and structure changes elsewhere.

Primary files:
- `docs/features/prompts.md`
- `docs/features/phases.md`
- `docs/features/runners.md`
- `docs/configuration.md`
- `docs/workflow.md`

Exit criteria for item 1:
- Shared execution and supervision use one recovery policy for known invalid-session fallbacks.
- Default prompts no longer hard-code environment-specific optional paths or tactics as universal requirements.
- Prompt tests validate invariants instead of large exact text fragments.
- Docs clearly distinguish hard contracts from preferred workflows.

### 2. Make Ralph feel like an operator console for nondeterministic agent runs, not a generic agent wrapper

Why second:
- Ralph's biggest product risk is not missing another model integration; it is leaving operators unsure what happened, what Ralph is doing now, and what the safest next action is after a nondeterministic run goes sideways.
- The highest-traffic product surfaces are already the run, recover, inspect, and repair loops: `ralph run one`, `ralph run loop`, `ralph run resume`, queue inspection commands, `ralph undo`, and the app's Queue, Run Control, Quick Actions, and task-detail flows.
- First run, resume, retry, and fresh re-invocation can diverge; Ralph has to narrate state, confidence, and operator choices explicitly instead of acting like a deterministic build tool.
- This comes after item 1 because clearer operator messaging works best once the underlying fallback semantics and prompt contracts are consistent.

Primary outcome:
- Operators should be able to tell, from the CLI or app, whether Ralph is resuming, rerunning fresh, waiting safely, blocked on a real invariant, or asking for a decision.

Detailed execution plan:

#### 2.1 Make resume state and fallback behavior legible
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

#### 2.2 Improve blocked / waiting / stalled-state visibility
- Differentiate:
  - dependency blocking
  - schedule blocking
  - lock contention
  - CI fallout
  - runner/session issues
  - true idle waiting.
- Keep queue/status/aging/burndown/app views consistent so they describe the same state model.

#### 2.3 Turn supervision errors into decision support
- Present detected CI patterns, retry state, and current escalation reason clearly.
- Make it obvious when `git_revert_mode` or runner/session capability is the real cause of the stop.
- Use the same decision language in CLI help, doctor output, and app messaging where practical.

#### 2.4 Normalize recovery tooling into the happy path
- Make `task mutate`, `task decompose`, `queue validate`, `queue repair`, and `undo` feel like normal continuation tools, not emergency escape hatches.
- Preserve partial value wherever safe instead of forcing operators into manual queue surgery.

#### 2.5 Tighten parallel only after serial recovery is boring
- Do not spend major churn on `run parallel` UX until serial run/resume/supervision behavior is calm and legible.
- When parallel work resumes, focus on bookkeeping visibility, stale lock handling, and post-run integration clarity.

Exit criteria for item 2:
- Operators can explain what Ralph is doing now and why, without reading source code.
- Session, waiting, and escalation messages are aligned across CLI and app surfaces.

### 3. Keep maintenance and structural hygiene active, but only when it helps the operator-facing roadmap move faster

Why third:
- Cleanup matters when it makes the run/recover/supervise surfaces safer to change, easier to validate, and easier to explain.
- Ralph already has the baseline architecture it needs; additional hygiene work should now be selective, evidence-driven, and tied to product-facing iteration speed.
- A small maintenance lane keeps test and fixture debt from slowing the UX work above without letting maintenance become the default priority again.

Primary outcome:
- Keep the repo easy to change in the areas the roadmap is actively using, without reopening broad cleanup churn.

Detailed execution plan:

#### 3.1 Split only where it improves failure locality or iteration speed
- Prioritize large suites and fixtures that slow operator-critical work.
- Keep `task_template_commands_test.rs` as the main known candidate, not an automatic next task.

#### 3.2 Keep profiling evidence-driven
- Re-profile before and after focused test work.
- Use target-specific nextest artifacts under `target/profiling/` instead of intuition-driven speed work.

#### 3.3 Keep environment isolation narrow and intentional
- Serialize tests that mutate shared PATH or process-global env.
- Avoid over-serializing tests that already use explicit runner overrides or isolated temp roots.

#### 3.4 Preserve real contract coverage on core operator paths
- Keep real `ralph init`, queue/undo/recovery, and supervision coverage where behavior correctness matters.
- Use cached or synthetic scaffolding only when the contract under test is not runtime fidelity.

#### 3.5 Maintain CLI / machine / app semantic parity
- Whenever operator-facing workflow changes land, verify equivalent semantics across:
  - human CLI
  - machine CLI
  - app integrations.

Exit criteria for item 3:
- Structural cleanup measurably reduces iteration pain on active roadmap paths.
- Maintenance work stays in service of product-facing clarity and reliability.

### 4. Add durable local timing visibility, then use it to tune the headless ship gate with evidence

Why fourth:
- Operator-facing improvements are easier to sustain when local validation speed regressions are visible instead of discovered by feel.
- Durable measurement keeps maintenance work and macOS gate tuning grounded in evidence instead of vibes.
- `macos-ci` is still expensive enough that any relaxation of serialization or job caps should be justified with data and protected by contract coverage.

Primary outcome:
- Validation performance should be observable locally, reproducible, and trendable before any CI-gate simplification decisions are made.

Detailed execution plan:

#### 4.1 Provide one clear local profiling entrypoint
- Add or document an opt-in command that writes machine-readable timing artifacts under `target/profiling/`.
- Keep the entrypoint headless and local-first.

#### 4.2 Measure the gates that actually influence iteration speed
- Capture timings for:
  - `make agent-ci`
  - targeted nextest suites for run/resume/supervision/undo/operator flows
  - doctests
  - `macos-build`
  - `macos-test`
  - `macos-test-contracts`.

#### 4.3 Separate Rust and Xcode timing stories
- Measure Xcode targets separately from Rust/CLI targets.
- Compare capped vs uncapped `RALPH_XCODE_JOBS` before changing defaults.

#### 4.4 Use evidence before changing serialization or job caps
- Do not relax xcodebuild serialization or default concurrency until profiling plus contract coverage show the tradeoff is safe.

Exit criteria for item 4:
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
