# Ralph Roadmap

Last updated: 2026-03-30

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Extract `migration/mod.rs` types into companion `types.rs` (488 LOC)

**Why first**: ~170 lines of data model before the first function. Per established facade pattern, types belong in a companion.

**Outcome**: `migration/types.rs` owns `Migration`, `MigrationType`, `MigrationStatus`, `MigrationCheckResult`, `MigrationContext`, and its builder. `mod.rs` is re-exports + dispatch only.

**Steps**:
- Move type definitions and `MigrationContext::build` to `types.rs`.
- Re-export from `mod.rs`.
- Update `use` paths in `registry.rs` and `history.rs` if needed.

**Exit criteria**:
- `migration/mod.rs` is re-exports + top-level dispatch.
- `migration/types.rs` owns data models.
- `make agent-ci` green.

**Files in scope**: `migration/mod.rs`, new `migration/types.rs`.

---

### 2. Remove dead-code wrappers in `prompts.rs`

**Why second**: Two `#[allow(dead_code)]` wrappers delegate to `prompts_internal::merge_conflicts` with zero callers. Trivial deletion; keep it after the split to avoid LOC audit noise.

**Outcome**: Remove the wrappers. Callers can import directly from `prompts_internal::merge_conflicts` if they ever appear.

**Steps**:
- Delete `load_merge_conflict_prompt` and `render_merge_conflict_prompt` from `prompts.rs`.
- Grep for call sites to confirm zero references.
- `make agent-ci` green.

**Exit criteria**:
- No `#[allow(dead_code)]` in `prompts.rs`.

**Files in scope**: `crates/ralph/src/prompts.rs`.

---

## Sequencing rules

- Keep completed work out of this file.
- Prefer one canonical operator path over wrappers, aliases, or repeated prose.
- Splits before deletions to keep LOC audits stable across batches.
- Batch 1 is an independent file-size split with no cross-dependencies.
- Batch 2 is a trivial deletion that can land any time after batch 1 (same module neighborhood).
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and `contracts/task`) while refactoring adjacent modules.
