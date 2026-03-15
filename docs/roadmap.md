# Ralph Roadmap

Last updated: 2026-03-14

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Split the remaining oversized macOS app/core orchestration files after the suite cutover

Why first:
- The persistence/parsing suite split is complete, but several macOS production files still sit above the file-size target and continue to mix multiple responsibilities.
- Queue/file-watching, runner control, settings infrastructure, and workspace presentation are now the most obvious app-side decomposition debt.
- With the latest Rust supervision coverage now broadened, the highest-churn remaining refactor target is back in the macOS production surface.

Scope:
- Decompose the current oversized macOS files (`QueueFileWatcher.swift`, `WorkspaceRunnerController.swift`, `ASettingsInfra.swift`, `AppSettings.swift`, `WorkspaceView.swift`, and `RunControlDetailSections.swift`) into thinner facades plus focused companion files.
- Preserve the current app/runtime/settings contracts and the recent noninteractive contract behavior while splitting responsibilities.
- Reuse shared support only when duplication is real; otherwise keep behavior-grouped companions adjacent to the facade.

### 2. Split the remaining oversized Rust operational modules after the app-side cutover

Why second:
- Recent supervision hardening widened regression coverage, but several Rust operational modules still exceed the file-size target and mix orchestration with helpers or test-only concerns.
- `parallel_worker.rs`, `git/commit.rs`, `commands/app.rs`, and adjacent CLI/task modules are now the clearest Rust-side decomposition hotspots.
- Keeping this after the macOS split avoids overlapping large Swift and Rust structural changes in the same verification window.

Scope:
- Decompose the current oversized Rust production files (`crates/ralph/src/commands/run/supervision/parallel_worker.rs`, `crates/ralph/src/git/commit.rs`, `crates/ralph/src/commands/app.rs`, and the next-largest adjacent operational modules) into thinner facades plus focused companion files.
- Preserve the newly expanded supervision/revert coverage while moving helpers and test-only seams out of the root modules.
- Keep behavior-grouped test hubs thin when splits require neighboring test-module moves.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed Settings/workspace-routing contract cutovers unless a new regression appears.
