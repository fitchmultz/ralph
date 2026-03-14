# Ralph Roadmap

Last updated: 2026-03-14

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Redesign macOS Settings smoke so routine local gates no longer hijack the desktop

Why first:
- `scripts/macos-settings-smoke.sh` is still a headed real-app workflow (`open -na ... --args --uitesting`), so routine local verification steals focus and interrupts normal workstation use.
- The crash-loop in `WorkspaceWindowAnchor.swift` is fixed and the Settings retarget regression is now covered again, so the remaining high-value follow-up is replacing the disruptive harness shape rather than revisiting the just-stabilized window retarget logic.
- Locking in a less disruptive Settings verification path before broader macOS lifecycle churn reduces the chance of reworking the same smoke coverage twice.

Scope:
- Decide whether the Settings smoke should stay as a headed script, move into XCTest-driven coverage, or use another deterministic in-process harness.
- Preserve the current keyboard/app-menu/URL-retarget coverage and workspace-specific config assertions.
- Eliminate routine desktop takeover and avoid crash-report/dialog noise during normal local gates.

### 2. Continue consolidating macOS workspace background-task ownership

Why second:
- The teardown-race cutover removed the noisy failures, but more workspace entrypoints still launch ad hoc background tasks.
- Post-run supervision coverage is now broadened, so the next highest-leverage churn reducer is finishing workspace task ownership.
- Completing ownership cleanup after the Settings smoke stabilization reduces the chance of reintroducing nondeterministic lifecycle bugs during app verification.

Scope:
- Audit remaining fire-and-forget workspace/bootstrap tasks for explicit ownership and cancellation.
- Prefer workspace-owned task slots over detached lifecycle work where repository context matters.
- Keep close/retarget/shutdown semantics deterministic across app and tests.

### 3. Split the remaining oversized macOS persistence and parsing suites after the lifecycle audit settles

Why third:
- `WindowStateTests.swift` remains above the file-size target and still mixes multiple persistence behaviors.
- `ANSIParserTests.swift` is near the soft limit and is a good candidate for behavior-focused decomposition once lifecycle churn subsides.
- Deferring this until after the ownership audit avoids re-splitting files that may still absorb lifecycle-driven test changes.

Scope:
- Break large persistence/parsing suites into behavior-focused files without changing coverage.
- Keep suite-level facade files thin and move reusable support into focused companions only when duplication is real.
- Preserve the current deterministic test-support entrypoints introduced by the recent cutovers.

### 4. Extend supervision hardening to parallel-worker and revert-mode edge cases

Why fourth:
- Standard post-run supervision now has broader lifecycle regression coverage, so the remaining higher-risk seams are worker-specific restore flows and revert/error branches.
- This should follow the macOS lifecycle audit so app/runtime churn does not mask supervision regressions during verification.
- Keeping this after the current Rust test cutover avoids mixing queue/git behavior expansions with the just-finished standard supervision coverage pass.

Scope:
- Add targeted coverage for parallel-worker bookkeeping restore failures, revert-mode inconsistency paths, and adjacent publish-mode/rebase surfaces not exercised by standard post-run tests.
- Keep runtime test modules behavior-grouped and thin at the root.
- Preserve the current cutover semantics; do not reintroduce compatibility branches.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed Settings window cutover unless a new regression appears.
