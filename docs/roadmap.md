# Ralph Roadmap

Last updated: 2026-03-13

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Split oversized macOS test support and runner-configuration suites after the fixture + lifecycle cutovers

Why first:
- The mock-fixture and teardown-race cutovers are now in place.
- `WorkspaceRunnerConfigurationTests.swift` still carries too many behaviors in one file.
- Smaller files will keep future macOS test churn localized and easier to review.

Scope:
- Break large RalphCore test suites into behavior-focused files without changing coverage.
- Keep `RalphCoreTestSupport.swift` and related helpers as thin facades over focused support files where needed.
- Preserve deterministic temp-fixture, shutdown, and queue-path helpers as the single source of truth.

### 2. Broaden post-run supervision regression coverage around adjacent lifecycle edges

Why second:
- The CI enforcement fix is now in place and green.
- Expanding coverage is safest after the macOS fixture churn above is reduced.
- This locks in the new supervision semantics before future run-loop or queue-lifecycle changes.

Scope:
- Add focused coverage for clean/dirty combinations around rejected tasks, already-archived done tasks, queue-maintenance repairs, and publish-mode variants.
- Keep post-run mutation/CI expectations explicit for both queue changes and repo changes.
- Guard the supervision refactor against future regressions without reopening the implementation design.

### 3. Continue consolidating macOS workspace background-task ownership

Why third:
- The current cutover removed the noisy teardown failures, but more workspace entrypoints still launch ad hoc background tasks.
- Finishing task-ownership cleanup after suite splitting will reduce future lifecycle regressions.
- This can proceed with lower churn once the large test files are decomposed.

Scope:
- Audit remaining fire-and-forget workspace/bootstrap tasks for explicit ownership and cancellation.
- Prefer workspace-owned task slots over detached lifecycle work where repository context matters.
- Keep close/retarget/shutdown semantics deterministic across app and tests.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed Settings window cutover unless a new regression appears.
