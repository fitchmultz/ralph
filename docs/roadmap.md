# Ralph Roadmap

Last updated: 2026-03-22

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Finish the remaining parallel operator-state gaps through shared runtime surfaces

Why first:
- The serial run/resume/recovery path is aligned enough for now; the biggest remaining operator confusion is in post-run parallel integration outcomes and app-side status parity.
- The remaining parallel gaps are post-run integration summaries and app consumption of the shared parallel status model.
- These fixes should stay on shared operator-state builders and continuation documents instead of creating more parallel-only wording paths.

Primary outcome:
- Parallel runs should explain integration outcomes and next steps from one shared operator-state model across CLI, machine, and app surfaces.

Detailed execution plan:

#### 1.1 Clarify post-run integration outcomes
- Turn merge/rebase/push/integration results into operator summaries instead of internal plumbing.
- Keep success, retryable failure, and operator-action-required cases structurally distinct.

#### 1.2 Project shared parallel status into app surfaces
- Feed the app from `MachineParallelStatusDocument` and shared continuation state instead of parallel-only app wording.
- Keep CLI, machine, and app status/recovery narration aligned from the same source.

Exit criteria for item 1:
- Parallel mode narrates integration outcomes consistently across CLI, machine, and app surfaces.
- The app consumes the same shared parallel-status model already used by CLI and machine output.
- New wording paths are shared-first, not parallel-only forks.

### 2. Capture real local timing baselines, then tune the ship gate only if the data justifies it

Why second:
- The profiling workflow is already documented; the missing step is collecting fresh baseline artifacts and using them to make decisions.
- Gate tuning without current measurements would create churn without confidence.
- Timing work is safer after the remaining shared parallel-status work stops moving operator-facing surfaces.

Primary outcome:
- Ship-gate tuning discussions should point to current local artifacts, not anecdotes.

Detailed execution plan:

#### 2.1 Record fresh baseline artifacts under `target/profiling/`
- Capture current timings for `make agent-ci`, targeted operator-path nextest suites, doctests, `macos-build`, `macos-test`, and `macos-test-contracts`.
- Keep the workflow headless and local-first.

#### 2.2 Compare Rust and Xcode costs separately
- Measure Rust/CLI and Xcode surfaces independently.
- Compare capped versus uncapped `RALPH_XCODE_JOBS` before changing defaults.

#### 2.3 Change concurrency or serialization only with evidence
- Do not relax xcodebuild serialization or default job caps unless profiling plus contract coverage show the tradeoff is safe.

Exit criteria for item 2:
- Timing artifacts exist for the gates that drive iteration speed.
- Any proposed ship-gate tuning is backed by fresh local data.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Prefer low-churn shared-runtime fixes before broader prompt, doc, or suite churn.
- Finish shared Rust/machine operator-state builders before app-only presentation follow-ups on the same path.
- Prefer operator-state clarity over maintenance-only cleanup when both are plausible next steps.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and contracts/task) while refactoring adjacent modules.
- Do not reopen completed serial recovery alignment, queue-lock recovery alignment, macOS test-defaults isolation, macOS Settings/workspace-routing cutovers, or git/init/app split work unless a new regression appears.
