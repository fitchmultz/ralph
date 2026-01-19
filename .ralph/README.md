# Ralph (Rust rewrite) runtime files

This repo is undergoing a Rust rewrite of Ralph. The Rust implementation uses the
`.ralph/` directory for repo-local state. See the root `README.md` for canonical
usage and migration guidance.

## Files

- `.ralph/queue.yaml` — YAML task queue (source of truth for active work).
- `.ralph/done.yaml` — YAML archive of completed tasks (same schema as queue).
- `ralph/prompts/` — optional prompt overrides (defaults are embedded in the Rust CLI).

## Minimal Rust Commands

- Validate queue:
  - `cargo run -p ralph -- queue validate`
- Bootstrap repo files (queue + done + config):
  - `cargo run -p ralph -- init`
- Inspect queue:
  - `cargo run -p ralph -- queue list`
  - `cargo run -p ralph -- queue list --status todo --tag rust`
  - `cargo run -p ralph -- queue show RQ-0008`
  - `cargo run -p ralph -- queue next --with-title`
- Next task ID:
  - `cargo run -p ralph -- queue next-id`
- Archive completed tasks:
  - `cargo run -p ralph -- queue done`
- Build a task from a request:
  - `cargo run -p ralph -- task build "<request>"`
- Seed tasks from a scan:
  - `cargo run -p ralph -- scan --focus "<focus>"`
- Run one task:
  - `cargo run -p ralph -- run one`
- Run multiple tasks:
  - `cargo run -p ralph -- run loop --max-tasks 0`

## Prompt Overrides

Defaults are embedded in the Rust CLI. To override prompts for this repo, create files under
`ralph/prompts/`:

- `ralph/prompts/worker.md`
- `ralph/prompts/task_builder.md`
- `ralph/prompts/scan.md`

Missing files fall back to the embedded defaults. Overrides must keep required placeholders.

## OpenCode Runner

Ralph can use the OpenCode CLI as a runner.

One-off usage:
- `cargo run -p ralph -- task build --runner opencode --model gpt-5.2 "Add tests for X"`
- `cargo run -p ralph -- scan --runner opencode --model gpt-5.2 --focus "CI gaps"`

Defaults via config (`.ralph/config.yaml` or `~/.config/ralph/config.yaml`):

```yaml
version: 1
agent:
  runner: opencode
  model: gpt-5.2
  opencode_bin: opencode
```

Allowed models: `gpt-5.2-codex`, `gpt-5.2`, `zai-coding-plan/glm-4.7`, `gemini-3-pro-preview`, `gemini-3-flash-preview`. Note: Codex
supports only `gpt-5.2-codex` and `gpt-5.2`; OpenCode accepts arbitrary model IDs.

## Supervisor Workflow (Rust)

`ralph run one` (and `ralph run loop`) act as a lightweight supervisor around the execution agent.

Core behavior:
- Task order is priority: the first `todo` in `.ralph/queue.yaml` is selected.
- The supervisor does NOT set `doing`; the agent does.
- Completed tasks should be moved from `.ralph/queue.yaml` to `.ralph/done.yaml`.
- Agents move completed tasks directly in the YAML files (not via `ralph queue done`).
- `ralph queue done` can be used to clean up any remaining `done` tasks in the queue.
- After the agent exits, the supervisor checks the repo state:
  - If the repo is clean and the task is `done` (archived), it proceeds to the next task.
  - If the repo is dirty, it runs `make ci`. On green, it commits + pushes all changes.
  - If the task is not `done`, the supervisor sets `done`, archives the task, and commits + pushes.

Common scenarios:
- Agent completes normally (done + archive + CI + commit + push) -> supervisor sees clean repo and moves on.
- Agent leaves dirty repo -> supervisor runs CI, archives, commits, pushes.
- Agent forgets to mark `done` -> supervisor sets `done`, archives, commits, pushes.

## Stress and Burn-In Tests

Stress tests live in `crates/ralph/tests/stress_queue_contract_test.rs` and include large-scale queue operations, archive/mutate cycles, and YAML repair stress.

Long-run burn-in is guarded by an env var so CI stays deterministic. The canonical way to run it is:
- `make stress` (runs in release mode with burn-in enabled)

Manual invocation:
- `RALPH_STRESS_BURN_IN=1 cargo test -p ralph --test stress_queue_contract_test -- --ignored --nocapture`

CI-safe stress coverage (already included in standard test runs):
- `cargo test -p ralph --test stress_queue_contract_test`

## Release Checklist

Before tagging a release or deploying to production:

1. **Clean Build & CI Gate**:
   - Run `make clean`
   - Run `make ci` (must pass 100%)
2. **Stress Verification**:
   - Run `make stress`
   - Ensure no panics or timeouts under load.
3. **Environment Check**:
   - Run `cargo run -p ralph -- doctor` (or `ralph doctor` if installed)
   - Verify all checks pass.
4. **Manual sanity check**:
   - `ralph queue list`
   - `ralph queue next`
