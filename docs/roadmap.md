# Ralph Roadmap

Last updated: 2026-03-28

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Thin the profiling implementation behind the Makefile entrypoints

Why next:
- `profile-ship-gate` is still one of the densest shell blocks in the `Makefile`.
- The current Makefile contract test still asserts profiling internals that will fight a clean helper extraction.

Primary outcome:
- The Makefile keeps thin profiling entrypoints while one focused helper owns orchestration, and tests assert the public contract instead of inline shell details.

Implementation steps:
- Move profiling orchestration into one dedicated helper while keeping `make profile-ship-gate` and `make profile-ship-gate-clean` as the operator-facing entrypoints.
- Preserve the artifact layout and exit behavior unless there is a strong reason to change them.
- Trim profiling contract coverage so it checks entrypoints, artifact paths, and cleanup behavior rather than helper implementation details.

Exit criteria:
- The Makefile profiling targets are thin wrappers.
- Profiling behavior stays unchanged from the operator’s point of view.
- Contract tests no longer pin inline Makefile shell implementation.

### 2. Relax brittle Makefile contract assertions after the gate surfaces settle

Why second:
- Current Makefile contract coverage still leans on exact help text and inline block text for some gate behaviors.
- After profiling helper boundaries stabilize, the remaining work is reducing avoidable test churn.

Primary outcome:
- Makefile contract tests focus on operator-visible behavior and target wiring instead of incidental wording.

Implementation steps:
- Audit the remaining Makefile contract assertions that depend on exact help prose or large inline recipe fragments.
- Keep strict assertions only where wording or inline structure is itself the public contract.
- Prefer assertions on target presence, delegated commands, artifact paths, and gate selection behavior.

Exit criteria:
- Routine help-text cleanup does not break unrelated contract tests.
- Contract coverage stays meaningful without pinning incidental implementation text.

## Sequencing rules

- Keep completed work out of this file.
- Prefer one canonical operator path over wrappers, aliases, or repeated prose.
- Stabilize profiling helper boundaries before relaxing remaining test brittleness around them.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and contracts/task) while refactoring adjacent modules.
