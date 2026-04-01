# Ralph Roadmap

Last updated: 2026-04-01

This is the canonical near-term roadmap for active follow-up work.
Source: comprehensive codebase audit (`docs/audits/codebase-audit-2026-03-31.md`)

## Active roadmap

### 1. Clone audit for runner/queue hot paths
- Identify unnecessary `String`/`Vec` clones in streaming and queue loading
- Consider `Cow<str>` or borrowing where lifetimes permit

### 2. Proactive decomposition of files in 400–500 LOC range
- `cli/scan.rs`, `cli/machine/task.rs`, `commands/init/writers.rs`, and 28 others
- Split before they breach the hard limit

---

## Sequencing rules

- Keep completed work out of this file.
- Prefer one canonical operator path over wrappers, aliases, or repeated prose.
- Prefer deleting dead wrappers before introducing new cleanup items in the same area.
- Preserve the hardened runtime split boundaries (`runutil/execution`, `runutil/retry`, `runutil/shell`, queue prune, fsutil, eta_calculator, undo, and `contracts/task`) while refactoring adjacent modules.
