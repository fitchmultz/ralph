# Ralph Roadmap

Last updated: 2026-03-23

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Finish the remaining RalphMac shared parallel-status surface audit

Why first:
- Run Control now renders the shared contract, but the rest of the app should either reuse that same operator-state summary or intentionally stay silent.
- Doing this before broader docs or performance work avoids another round of parallel-status wording churn.

Primary outcome:
- RalphMac should have one deliberate answer for where shared parallel operator state appears outside Run Control.

Detailed execution plan:

#### 1.1 Audit recovery and diagnostics surfaces
- Review `ErrorRecoveryView`, workspace diagnostics, and any operator-facing run summaries for overlapping parallel-state messaging.
- Identify places that should consume the shared contract versus places that should defer to Run Control.

#### 1.2 Reuse the shared model or remove duplicate wording
- Where parallel status belongs, render the existing shared continuation/blocking summary.
- Where it does not belong, remove ad hoc or duplicate parallel wording instead of inventing another app-local status model.

#### 1.3 Add only the minimum confirming coverage
- Add focused app/state coverage for any retained surface.
- Skip broad UI churn if the audit concludes Run Control is the sole canonical surface.

Exit criteria for item 1:
- Remaining operator-facing RalphMac surfaces have an explicit, consistent parallel-status policy.
- No duplicate app-local parallel-state narration remains in touched surfaces.

### 2. Capture real local timing baselines, then tune the ship gate only if the data justifies it

Why second:
- The profiling workflow is already documented; the missing step is collecting fresh baseline artifacts and using them to make decisions.
- Gate tuning without current measurements would create churn without confidence.
- Timing data is more useful after the immediate RalphMac follow-up noise is removed.

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
- Prefer low-churn shared-runtime and app-contract cleanup before broader prompt, doc, or suite churn.
- Keep RalphMac parallel-status follow-up work anchored to the shared `machine run parallel-status` contract; do not fork app-local status logic.
- Prefer current measurement artifacts over anecdotal gate-tuning claims.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and contracts/task) while refactoring adjacent modules.
- Do not reopen completed serial recovery alignment, queue-lock recovery alignment, macOS test-defaults isolation, macOS Settings/workspace-routing cutovers, or git/init/app split work unless a new regression appears.
