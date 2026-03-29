# Ralph Roadmap

Last updated: 2026-03-28

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Finish collapsing macOS operator guidance onto one canonical doc path

Why next:
- `Makefile` help, `docs/index.md`, `docs/features/app.md`, `docs/troubleshooting.md`, and `docs/guides/ci-strategy.md` still overlap on macOS validation, profiling, and UI-evidence guidance.
- The profiling semantics are now stable enough that the remaining work is doc-surface cleanup only.

Primary outcome:
- The macOS CI/profile/UI-artifact workflow has one primary home, with short pointers elsewhere.

Implementation steps:
- Choose the canonical operator doc for macOS validation, profiling, and UI evidence capture.
- Trim secondary surfaces to one-line pointers or short examples only.
- Remove wording that duplicates the shipped profiling and cleanup contract.

Exit criteria:
- The same macOS workflow is no longer described in multiple places with different levels of detail.
- Secondary docs stay short and non-conflicting.

### 2. Thin the profiling implementation behind the Makefile entrypoints

Why second:
- `profile-ship-gate` is still one of the densest shell blocks in the `Makefile`.
- With the profiling semantics fixed, the remaining churn is now pure maintainability cleanup.

Primary outcome:
- The Makefile keeps thin profiling entrypoints while the orchestration lives in one focused helper.

Implementation steps:
- Move profiling orchestration into one dedicated helper while keeping `make profile-ship-gate` and `make profile-ship-gate-clean` as the operator-facing entrypoints.
- Preserve the artifact layout and exit behavior unless there is a strong reason to change them.
- Keep test coverage focused on the public entrypoints and artifact contract, not duplicated implementation details.

Exit criteria:
- The Makefile profiling targets are thin wrappers.
- Profiling behavior stays unchanged from the operator’s point of view.

## Sequencing rules

- Keep completed work out of this file.
- Prefer one canonical operator path over wrappers, aliases, or repeated prose.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and contracts/task) while refactoring adjacent modules.
