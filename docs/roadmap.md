# Ralph Roadmap

Last updated: 2026-03-25

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Make macOS local validation interruption-safe and operator-truthful

Why first:
- Interrupted local runs can still strand `target/tmp/locks/xcodebuild.lock` and related project-owned artifacts.
- Repo guidance still has at least one stale macOS UI artifact description, so the workflow is not fully self-consistent yet.

Primary outcome:
- macOS local validation recovers cleanly after interruption, and every remaining operator-facing surface describes the shipped workflow accurately.

Implementation steps:
- Audit project-owned lock, temp, and derived-data artifacts created by Makefile macOS targets and contract scripts.
- Add stale-lock detection and recovery for `target/tmp/locks/xcodebuild.lock` before wait paths block indefinitely.
- Align remaining macOS UI artifact guidance with the shipped `.xcresult` + summary workflow; remove stale wording instead of adding caveats.
- Keep cleanup scoped to clearly project-owned artifacts and preserve the existing loud-failure contract for lingering app processes.
- Add or trim deterministic contract coverage only where it protects the recovery path and the operator contract.

Exit criteria:
- Manual deletion of stranded macOS validation artifacts is no longer needed after interrupted local runs.
- Remaining Makefile/help/doc surfaces describe the same macOS validation workflow.
- Contract coverage catches regressions in stale-lock handling and cleanup recovery.

### 2. Refresh ship-gate baselines and settle local concurrency defaults

Why second:
- `target/profiling/` still reflects pre-stabilization runs from 2026-03-16/17.
- `RALPH_XCODE_JOBS` should only move after one fresh local baseline pass on the stabilized workflow.

Primary outcome:
- One current profiling set exists for the CLI + RalphMac ship gate, and local concurrency defaults are either updated from that data or explicitly kept.

Implementation steps:
- Re-run `make agent-ci`, doctests, targeted operator-path nextest suites, `macos-build`, `macos-test`, and `macos-test-contracts` under comparable local conditions.
- Replace stale timing outputs in `target/profiling/` with one current naming scheme and a short summary of the slowest surfaces.
- Compare capped versus uncapped `RALPH_XCODE_JOBS` runs and change defaults only if the win is material and contract coverage stays stable.

Exit criteria:
- `target/profiling/` contains one current baseline set instead of mixed cutover history.
- Any default change is justified by fresh measurements, or the current defaults are explicitly reaffirmed.

### 3. Collapse redundant macOS UI artifact surface area if it still does not earn its own target

Why third:
- After the ship cutover, `macos-test-ui-artifacts` is mostly a thin wrapper around `macos-ui-retest` plus timestamped bundle preservation.
- Keep the extra entrypoint only if it materially improves local review workflow after interruption handling and baseline refresh are settled.

Primary outcome:
- The repo keeps either one clear macOS UI artifact workflow or one clearly justified wrapper target, not both by accident.

Implementation steps:
- Re-evaluate whether `macos-test-ui-artifacts` adds enough value beyond `macos-ui-retest` with an explicit result-bundle path.
- If it does not, remove the redundant target/docs/contracts and keep one canonical invocation path.
- If it does, keep the wrapper but trim any remaining duplicated instructions around the same workflow.

Exit criteria:
- macOS UI artifact capture has one canonical operator-facing path.
- Redundant wrapper/help/doc/test surface area is removed or explicitly justified.

## Sequencing rules

- Keep completed work out of this file.
- Roadmap items must be chunky, dependency-aware work packages; combine adjacent evidence, cleanup, and tuning work instead of splitting follow-ups into trivial single-step tasks.
- Stabilize interruption handling and operator truthfulness before refreshing baselines.
- Refresh measurements before revisiting local concurrency defaults.
- Only decide whether to keep the extra UI artifact target after the underlying workflow is stable and measured.
- Keep shared `machine run parallel-status` decoding and version checks in RalphCore; keep Run Control presentation-only.
- Keep Run Control's initial `.task` refresh on the status-only path; use full refresh only when queue or task data must change.
- Prefer current measurement artifacts over anecdotal gate-tuning claims.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and contracts/task) while refactoring adjacent modules.
- Do not reopen completed serial recovery alignment, queue-lock recovery alignment, macOS test-defaults isolation, macOS Settings/workspace-routing cutovers, git/init/app split work, macOS test-cleanup hardening, or the removed xcresult-attachment export path unless a new regression appears.
