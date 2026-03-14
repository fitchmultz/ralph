# Ralph Roadmap

Last updated: 2026-03-14

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Broaden noninteractive macOS contract coverage beyond the Settings cutover

Why first:
- The remaining workspace-owned lifecycle tasks are now explicitly owned and cancellable, so the next best leverage is moving more fragile app flows off headed-only verification.
- The shared presentation runtime is now the enforced path for CI-facing app launches, which makes additional contract cutovers lower-risk and easier to keep noninteractive.
- Expanding deterministic in-process contract coverage now should catch lifecycle regressions before they drift back behind UI-only smoke.

Scope:
- Identify app flows that still depend on headed-only verification and migrate the highest-value ones to deterministic in-process contracts.
- Reuse the offscreen presentation/runtime helpers instead of adding new one-off harnesses.
- Keep contract reports machine-readable so `make macos-ci` can stay noninteractive by default.

### 2. Split the remaining oversized macOS persistence and parsing suites after the lifecycle audit settles

Why second:
- `WindowStateTests.swift` remains above the file-size target and still mixes multiple persistence behaviors.
- `ANSIParserTests.swift` is near the soft limit and is a good candidate for behavior-focused decomposition once lifecycle churn subsides.
- Deferring this until after the latest lifecycle/contract hardening avoids re-splitting files that may still absorb follow-on macOS contract changes.

Scope:
- Break large persistence/parsing suites into behavior-focused files without changing coverage.
- Keep suite-level facade files thin and move reusable support into focused companions only when duplication is real.
- Preserve the current deterministic test-support entrypoints introduced by the recent cutovers.

### 3. Extend supervision hardening to parallel-worker and revert-mode edge cases

Why third:
- Standard post-run supervision now has broader lifecycle regression coverage, so the remaining higher-risk seams are worker-specific restore flows and revert/error branches.
- Keeping this after the current macOS lifecycle/contract work reduces the chance of mixing app-runtime churn with queue/git behavior expansions during verification.
- The recent Rust test cutover makes these remaining worker/revert seams the next most obvious supervision gaps.

Scope:
- Add targeted coverage for parallel-worker bookkeeping restore failures, revert-mode inconsistency paths, and adjacent publish-mode/rebase surfaces not exercised by standard post-run tests.
- Keep runtime test modules behavior-grouped and thin at the root.
- Preserve the current cutover semantics; do not reintroduce compatibility branches.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed Settings window cutover unless a new regression appears.
