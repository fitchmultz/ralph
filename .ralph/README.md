# Ralph (Rust rewrite) runtime files

This repo is undergoing a Rust rewrite of Ralph. The Rust implementation uses the
`.ralph/` directory for repo-local state. See the root `README.md` for canonical
usage and migration guidance.

## Files

- `.ralph/queue.yaml` — YAML task queue (source of truth for active work).
- `.ralph/done.yaml` — YAML archive of completed tasks (same schema as queue).
- `.ralph/prompts/` — optional prompt overrides used by the runner.

## Minimal Rust Commands

- Validate queue:
  - `cargo run -p ralph -- queue validate`
- Bootstrap repo files (queue + config):
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
- `status: blocked` is not supported. If encountered, the supervisor reverts uncommitted changes
  (if any) and stops.

Common scenarios:
- Agent completes normally (done + archive + CI + commit + push) -> supervisor sees clean repo and moves on.
- Agent leaves dirty repo -> supervisor runs CI, archives, commits, pushes.
- Agent forgets to mark `done` -> supervisor sets `done`, archives, commits, pushes.

## Legacy (Go) Ralph

The existing Go-based implementation still uses:

- `.ralph/ralph.json`
- `.ralph/pin/`

Those files remain in the repo during migration but are not part of the Rust
queue contract.
