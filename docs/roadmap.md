# Ralph Roadmap

Last updated: 2026-04-01

This is the canonical near-term roadmap for active follow-up work.
Source: comprehensive codebase audit (`docs/audits/codebase-audit-2026-03-31.md`)

## Active roadmap

### 1. Split next 3 production files in 400–500 LOC range
- `commands/task/decompose/support.rs`, `commands/init/readme.rs`, `commands/task/mod.rs`
- Why: these are now the largest remaining non-test Rust files still near the hard limit, and each has obvious structural split seams without requiring behavior changes.

---

## Sequencing rules

- Keep completed work out of this file.
- Prefer one canonical operator path over wrappers, aliases, or repeated prose.
- Prefer deleting dead wrappers before introducing new cleanup items in the same area.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and `contracts/task`) while refactoring adjacent modules.
