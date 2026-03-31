# Ralph Roadmap

Last updated: 2026-03-30

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Remove dead-code wrappers in `prompts.rs`

**Why next**: Two `#[allow(dead_code)]` wrappers delegate to `prompts_internal::merge_conflicts` with zero callers. Trivial deletion now that the migration split landed.

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
- Prefer deleting dead wrappers before introducing new cleanup items in the same area.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and `contracts/task`) while refactoring adjacent modules.
