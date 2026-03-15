# Ralph Roadmap

Last updated: 2026-03-15

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Split the remaining oversized Rust command and CLI surfaces

Why first:
- The current source scan still reports 43 source files over the 500 LOC target, and the highest-churn hotspots remain command-routing and CLI surfaces.
- Command/CLI facades still mix routing, validation, formatting, and workflow orchestration in single files.
- Decomposing these entrypoints first reduces double-moves before deeper runtime helpers are split.

Scope:
- Decompose the remaining oversized command and CLI modules (`crates/ralph/src/commands/task/decompose/mod.rs`, `crates/ralph/src/commands/task/update.rs`, `crates/ralph/src/commands/context/wizard.rs`, and adjacent command helpers) into thinner facades plus focused companion files.
- Preserve current CLI/help output, prompt behavior, and queue/task contracts while moving helpers and formatting logic out of the root modules.
- Keep any moved test hubs thin and behavior-grouped when command splits require neighboring test-module moves.

### 2. Split the remaining oversized Rust runtime and operational helpers

Why second:
- After command entrypoints stabilize, the next maintenance risk is the runtime/support layer that still mixes orchestration, persistence, retries, and formatting.
- Webhook, queue-maintenance, processor execution, filesystem helpers, and execution-history modules remain broad enough to create avoidable churn during feature work.
- Sequencing this pass after the command split limits cross-cutting rename churn.

Scope:
- Decompose the remaining oversized operational helpers (`crates/ralph/src/webhook/worker.rs`, `crates/ralph/src/webhook/diagnostics.rs`, `crates/ralph/src/queue/prune.rs`, `crates/ralph/src/queue/hierarchy.rs`, `crates/ralph/src/plugins/processor_executor.rs`, `crates/ralph/src/runutil/execution/orchestration.rs`, `crates/ralph/src/fsutil.rs`, `crates/ralph/src/execution_history.rs`, and adjacent support modules) into focused companions.
- Preserve webhook reload/retry contracts, queue safety behavior, and managed-subprocess invariants while extracting helpers from the root modules.
- Keep shared helpers centralized only where duplication is real; otherwise prefer adjacent behavior-grouped modules.

### 3. Split the remaining oversized Rust shared-data and foundational modules

Why third:
- Foundational helpers such as migration, template, agent-resolution, redaction, ETA, undo, and task-contract modules are broadly reused, so touching them earlier would amplify churn.
- Once command/runtime seams are thinner, the dependency picture is clearer and shared-module splits become lower-risk.
- These files remain important debt, but they are lower-churn than the active command/runtime hotspots.

Scope:
- Decompose the remaining oversized foundational modules (`crates/ralph/src/migration/config_migrations.rs`, `crates/ralph/src/migration/file_migrations.rs`, `crates/ralph/src/template/variables.rs`, `crates/ralph/src/template/loader.rs`, `crates/ralph/src/agent/resolve.rs`, `crates/ralph/src/redaction.rs`, `crates/ralph/src/eta_calculator.rs`, `crates/ralph/src/undo.rs`, `crates/ralph/src/contracts/task.rs`, and adjacent shared helpers) into thinner facades plus focused companions.
- Preserve schema, normalization, redaction, and task-contract behavior exactly while moving parsing/formatting helpers out of the root files.
- Prefer deterministic helper modules and avoid reopening stabilized command/runtime seams unless a true shared abstraction emerges.

### 4. Split the remaining oversized Rust test and fixture hubs

Why fourth:
- Once production-module facades are thinner, the largest remaining non-doc maintenance debt sits in integration/unit suites and shared test-support hubs.
- Large files such as `task_lifecycle_test.rs`, `run_parallel_test.rs`, `prompt_cli_test.rs`, `phase_settings_matrix.rs`, and queue-operation suites remain clear follow-on churn hotspots.
- Sequencing test-hub splits after production refactors minimizes duplicate test moves while contracts are still settling.

Scope:
- Break remaining oversized Rust test and fixture files into thin suite roots plus behavior-grouped companions/directories.
- Preserve current coverage names, helper contracts, and `make agent-ci` / `make ci` verification behavior.
- Keep shared test support centralized only where duplication is real; otherwise prefer adjacent grouped helpers.

## Sequencing rules

- Keep completed roadmap items out of this file; replace them with the next active work only.
- Preserve the recently hardened bundle/versioning/plugin/tutorial contracts while refactoring adjacent modules.
- Prefer infrastructure and fixture stabilization before broader feature churn.
- Do not reopen the completed macOS Settings/workspace-routing or the completed git/init/app split cutovers unless a new regression appears.
