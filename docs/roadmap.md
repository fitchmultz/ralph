# Ralph Roadmap

Last updated: 2026-03-13

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Centralize macOS mock CLI fixture generation and resolved-path payloads

Why first:
- Recent macOS test failures came from drift in inline mock CLI scripts and fake machine payloads.
- Shared fixture builders will reduce churn before more workspace/app-surface work lands.
- This keeps path, queue, and config payload contracts consistent across RalphCore tests.

Scope:
- Move repeated mock CLI script and JSON payload construction into shared RalphCore test support.
- Ensure machine queue/config payloads always emit real workspace-resolved paths.
- Remove ad hoc per-test placeholder payloads where possible.

### 2. Reduce macOS test noise from fixture teardown and async refresh races

Why second:
- Once fixtures are centralized, the remaining failures/noise are easier to isolate as lifecycle issues instead of payload bugs.
- Current passing test runs still emit benign-but-noisy runner-configuration failures after temporary fixture executables are removed.
- Quieting that noise will make real regressions easier to spot.

Scope:
- Prevent background runner-config or watcher refresh work from outliving test fixtures.
- Tighten workspace/test teardown so temporary CLI binaries and temp directories are not observed after cleanup.
- Keep operational-health diagnostics meaningful instead of flooding logs with expected teardown errors.

### 3. Broaden post-run supervision regression coverage around adjacent lifecycle edges

Why third:
- The CI enforcement fix is now in place and green.
- Expanding coverage is safest after the macOS fixture churn above is reduced.
- This locks in the new supervision semantics before future run-loop or queue-lifecycle changes.

Scope:
- Add focused coverage for clean/dirty combinations around rejected tasks, already-archived done tasks, queue-maintenance repairs, and publish-mode variants.
- Keep post-run mutation/CI expectations explicit for both queue changes and repo changes.
- Guard the supervision refactor against future regressions without reopening the implementation design.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed Settings window cutover unless a new regression appears.
