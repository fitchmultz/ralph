# Ralph (Rust rewrite) runtime files

This repo is undergoing a Rust rewrite of Ralph. The Rust implementation uses the
`.ralph/` directory for repo-local state. See the root `README.md` for canonical
usage and migration guidance.

## Files

- `.ralph/queue.yaml` — YAML task queue (source of truth for active work).
- `.ralph/done.yaml` — YAML archive of completed tasks (same schema as queue).
- `.ralph/prompts/` — optional prompt overrides (defaults are embedded in the Rust CLI).

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
`.ralph/prompts/`:

- `.ralph/prompts/worker.md`
- `.ralph/prompts/task_builder.md`
- `.ralph/prompts/scan.md`

Missing files fall back to the embedded defaults. Overrides must keep required placeholders.

## Runners (OpenCode + Gemini + Claude)

Ralph can use the OpenCode, Gemini, or Claude CLI as a runner.

One-off usage:
- `cargo run -p ralph -- task build --runner opencode --model gpt-5.2 "Add tests for X"`
- `cargo run -p ralph -- scan --runner opencode --model gpt-5.2 --focus "CI gaps"`
- `cargo run -p ralph -- scan --runner gemini --model gemini-3-flash-preview --focus "risk audit"`
- `cargo run -p ralph -- scan --runner claude --model sonnet --focus "risk audit"`
- `cargo run -p ralph -- task build --runner claude --model opus "Add tests for X"`

Defaults via config (`.ralph/config.yaml` or `~/.config/ralph/config.yaml`):

```yaml
version: 1
agent:
  runner: opencode
  model: gpt-5.2
  opencode_bin: opencode
  gemini_bin: gemini
  claude_bin: claude
  two_pass_plan: true
```

**Allowed models by runner:**
- **Codex**: `gpt-5.2-codex`, `gpt-5.2` (only these two)
- **OpenCode**: arbitrary model IDs (e.g., `zai-coding-plan/glm-4.7`)
- **Gemini**: `gemini-3-pro-preview`, `gemini-3-flash-preview`, or arbitrary IDs
- **Claude**: `sonnet` (default), `opus`, or arbitrary model IDs

**Two-pass plan mode**: When enabled (`two_pass_plan: true`), Claude first generates a plan in plan mode, then implements it with auto-approval. This provides better structure and visibility into planned changes. If plan generation fails, falls back to direct implementation. Currently supported for Claude runner only; will expand to OpenCode in the future.

Gemini runner prepends a RepoPrompt tooling instruction at the top of every prompt.

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
