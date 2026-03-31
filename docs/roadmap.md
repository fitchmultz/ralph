# Ralph Roadmap

Last updated: 2026-03-30

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Deduplicate macOS task-mutation encoding and clean up portable-path test debt

**Why next**: The app still hand-assembles task field edits with stringly-typed field names while some tests continue to hardcode `/tmp` paths despite portable temp helpers.

**Outcome**: Task mutation encoding becomes centrally defined, and test fixtures stop depending on Unix-only temp paths.

**Steps**:
- Introduce one shared field-to-edit encoder for `Workspace+TaskMutations.swift` flows.
- Add focused coverage for multi-field diff generation, not just agent overrides.
- Replace hardcoded `/tmp` test paths with temp-root helpers in affected Rust/Swift tests.
- Re-run the relevant local gate.

**Exit criteria**:
- Task mutation field encoding is not duplicated across single-field and bulk edit flows.
- Audited tests no longer require literal `/tmp` paths.

**Files in scope**: `apps/RalphMac/RalphCore/Workspace+TaskMutations.swift`, `apps/RalphMac/RalphCoreTests/ErrorRecoveryCategoryTests.swift`, `crates/ralph/src/commands/app/tests.rs`, related tests.

---

## Sequencing rules

- Keep completed work out of this file.
- Prefer one canonical operator path over wrappers, aliases, or repeated prose.
- Prefer deleting dead wrappers before introducing new cleanup items in the same area.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and `contracts/task`) while refactoring adjacent modules.
