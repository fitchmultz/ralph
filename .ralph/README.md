# Ralph (Rust rewrite) runtime files

This repo is undergoing a Rust rewrite of Ralph. The Rust implementation uses the
`.ralph/` directory for repo-local state.

## Files

- `.ralph/queue.yaml` — YAML task queue (source of truth for active work).
- `.ralph/prompts/` — optional prompt overrides used by the runner.

## Legacy (Go) Ralph

The existing Go-based implementation still uses:

- `.ralph/ralph.json`
- `.ralph/pin/`

Those files remain in the repo during migration but are not part of the Rust
queue contract.