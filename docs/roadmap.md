# Ralph Roadmap

Last updated: 2026-03-30

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Split `prompts_internal/util.rs` (627 → <500 LOC)

**Why first**: Largest file in `prompts_internal/`. Two clear halves — instruction-file I/O (lines 90–280) and template-variable expansion (lines 280–627).

**Outcome**: Instruction-file helpers move to `prompts_internal/instructions.rs`. `util.rs` keeps template expansion, validation, and project-type guidance.

**Steps**:
- Move `resolve_instruction_path`, `read_instruction_file`, `wrap_with_instruction_files`, `instruction_file_warnings`, `validate_instruction_file_paths` into `instructions.rs`.
- Re-export from `mod.rs`.
- Move their inline tests to `instructions.rs`.

**Exit criteria**:
- Both files under 500 LOC.
- `make agent-ci` green.

**Files in scope**: `prompts_internal/util.rs`, new `prompts_internal/instructions.rs`, `prompts_internal/mod.rs`.

---

### 2. Split `cli/machine/queue.rs` (610 → <500 LOC)

**Why second**: Handler + three independent document builders. The doc builders (validate, repair, undo) each produce a self-contained `Machine*Document` and share no helpers with each other.

**Outcome**: `handle_queue` stays in `queue.rs`. The three document builders move to `cli/machine/queue_docs.rs`.

**Steps**:
- Move `build_validate_document`, `build_repair_document`, `build_undo_document`, and their private helpers (`queue_validation_failed_state`, `repair_preview_continuation`, `continuation_for_valid_queue`, `step`, `build_graph_json`) into `queue_docs.rs`.
- Re-export from `cli/machine/mod.rs`.
- Update the CLI queue/undo callers that import through `crate::cli::machine::build_*`.

**Exit criteria**:
- Both files under 500 LOC.
- `make agent-ci` green.

**Files in scope**: `cli/machine/queue.rs`, new `cli/machine/queue_docs.rs`, `cli/machine/mod.rs`.

---

### 3. Remove dead-code wrappers in `prompts.rs`

**Why third**: Two `#[allow(dead_code)]` wrappers delegate to `prompts_internal::merge_conflicts` with zero callers. Trivial deletion; placed after splits to avoid merge noise.

**Outcome**: Remove the wrappers. Callers can import directly from `prompts_internal::merge_conflicts` if they ever appear.

**Steps**:
- Delete `load_merge_conflict_prompt` and `render_merge_conflict_prompt` from `prompts.rs`.
- Grep for call sites to confirm zero references.
- `make agent-ci` green.

**Exit criteria**:
- No `#[allow(dead_code)]` in `prompts.rs`.

**Files in scope**: `crates/ralph/src/prompts.rs`.

---

### 4. Extract `migration/mod.rs` types into companion `types.rs` (488 LOC)

**Why fourth**: ~170 lines of data model before the first function. Per established facade pattern, types belong in a companion.

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

## Sequencing rules

- Keep completed work out of this file.
- Prefer one canonical operator path over wrappers, aliases, or repeated prose.
- Splits before deletions to keep LOC audits stable across batches.
- Batches 1–3 are independent file-size splits with no cross-dependencies.
- Batch 4 is a trivial deletion that can land any time after batch 2 (same module neighborhood).
- Batch 5 is independent of all others.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and `contracts/task`) while refactoring adjacent modules.
