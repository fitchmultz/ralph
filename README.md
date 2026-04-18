# Ralph

[![crates.io](https://img.shields.io/crates/v/ralph-agent-loop.svg)](https://crates.io/crates/ralph-agent-loop)
[![docs.rs](https://img.shields.io/docsrs/ralph-agent-loop)](https://docs.rs/ralph-agent-loop)
[![GitHub Release](https://img.shields.io/github/v/release/fitchmultz/ralph)](https://github.com/fitchmultz/ralph/releases)

Ralph is a local-first AI coding workflow tool with a Rust CLI and a SwiftUI macOS app, both built around a structured task queue stored in your repository.

Teams use Ralph when ad-hoc AI coding stops being enough and they need a repeatable way to turn requests into queued work, run that work through Codex/Claude/Gemini-style agents, and keep the result reviewable with local files, local CI, and explicit task history instead of hidden SaaS state.

## Reviewer Path

If you are evaluating the repo quickly and want the fastest high-signal path:

1. Read the product overview in this README.
2. Run the no-runner-required verification flow in [docs/guides/local-smoke-test.md](docs/guides/local-smoke-test.md).
3. Skim the command map in [docs/cli.md](docs/cli.md).
4. Use [docs/guides/evaluator-path.md](docs/guides/evaluator-path.md) for a short "what to try, what to expect" walkthrough.

That path is intentionally local-first and does not require configuring Codex/Claude/Gemini before you can validate the repo.

## What Ralph Is For

Ralph is designed for engineering teams that want repeatable, auditable AI-assisted development workflows.

It provides:

- A structured task queue with explicit lifecycle and dependency links
- Multi-runner execution (`codex`, `opencode`, `gemini`, `claude`, `cursor`, `kimi`, `pi`)
- Supervised 1/2/3-phase execution (plan, implement, review)
- Parallel execution with workspace isolation
- Guardrails around queue validity, retries, session recovery, and local CI gates

### Non-goals

- Hosted SaaS orchestration (Ralph is local-first)
- Hidden black-box state (queue and done files are plain JSONC in `.ralph/`)
- Replacing your existing developer tooling; Ralph integrates with it

## Architecture at a Glance

```mermaid
flowchart LR
  APP["macOS App<br>SwiftUI"] -->|shells out| CLI["ralph CLI<br>Rust"]
  CLI -->|reads/writes| QUEUE[.ralph/queue.jsonc]
  CLI -->|reads/writes| DONE[.ralph/done.jsonc]
  CLI -->|reads| CONFIG[.ralph/config.jsonc]
  CLI -->|spawns| RUNNERS["Runner CLIs<br>Codex / Claude / Gemini / OpenCode / Cursor / Kimi / Pi"]
```

## Install

From crates.io:

```bash
cargo install ralph-agent-loop
```

This installs the `ralph` executable.

From source:

> GNU Make >= 4 is required for project targets. On macOS, install via `brew install make` and use `gmake` unless GNU Make is already your default `make`.

```bash
git clone https://github.com/fitchmultz/ralph
cd ralph
make install
# macOS/Homebrew GNU Make users: gmake install
```

## Supported Platforms & Toolchain

- Supported OS: macOS and Linux
- Rust toolchain: pinned by `rust-toolchain.toml` (for deterministic fmt/clippy/test behavior)
- SwiftUI app: macOS only (`apps/RalphMac/`)

## Quick Start

```bash
# 1) Initialize in your repo
ralph init

# 2) Inspect the default-safe profile
ralph config profiles

# 3) Add a task
ralph task "Stabilize flaky queue integration test"

# 4) Execute one task with the recommended safe profile
ralph run one --profile safe

# 5) Inspect queue state
ralph queue list
```

`ralph init` now defaults to the safe path: non-aggressive approvals, no automatic git publish, and parallel execution kept opt-in.
Use `--profile power-user` only when you explicitly want the higher-blast-radius behavior, including commit_and_push automation.
On macOS, app-launched runs remain noninteractive: the app can supervise and disclose safety posture, but interactive approvals are still terminal-only.

If you do not want to configure a runner yet, use the smoke-test flow instead of `ralph run one`.
That gives you a deterministic way to verify the CLI and repo health without any external model setup.

## End-to-End Example

Here is a concrete repo workflow for a team using Codex or Claude Code in a normal feature branch:

```bash
# install Ralph in your application repo
cargo install ralph-agent-loop
cd your-service
ralph init

# turn a real request into queued work
ralph task "Add retry coverage for webhook delivery failures"

# inspect the task Ralph just created
ralph queue list
ralph queue show RQ-0001

# let your configured runner plan, implement, and review the task
ralph run one --profile safe --phases 3

# verify the repo is still healthy and the task moved forward
ralph queue list
ralph doctor
```

What this gives the team: one tracked queue, one explicit task lifecycle, one local verification path, and the flexibility to swap runners without changing the repo workflow.

## Local Smoke Test (5 minutes)

No external runner setup required:

```bash
ralph init
ralph --help
ralph help-all
ralph run one --help
ralph scan --help
ralph queue list
ralph queue graph
ralph queue validate
ralph doctor
make agent-ci
```

Expected signals:

- Help and queue commands succeed
- `ralph doctor` exits successfully
- `make agent-ci` completes with passing checks for the current dependency surface
- Source snapshots without `.git/` fall back to `make release-gate` (`macos-ci` on macOS with Xcode, otherwise `ci`)

Full scripted version: [docs/guides/local-smoke-test.md](docs/guides/local-smoke-test.md)

## Security & Data Handling

Ralph is local-first, but selected runner CLIs may transmit prompts/context to external APIs depending on your runner configuration.

- Do not place secrets in task text, notes, or tracked config
- Keep runtime artifacts local (`.ralph/cache/`, `.ralph/logs/`, `.ralph/workspaces/`, `.ralph/undo/`, `.ralph/webhooks/`)
- Use `make pre-public-check` before public release windows

Security references:

- [SECURITY.md](SECURITY.md)
- [Security Model](docs/security-model.md)

## Known Limitations

- Quality/speed depend on selected runner model and prompts
- UI tests are intentionally not part of default `make macos-ci` (headed interaction)
- Parallel execution is experimental and introduces additional branch/workspace complexity in very large repos

## Versioning & Compatibility

Ralph follows semantic versioning.

- Minor/patch releases preserve existing behavior unless explicitly documented
- Breaking CLI/config behavior changes are called out in changelog and migration notes

Details: [docs/versioning-policy.md](docs/versioning-policy.md)

## Documentation

Start here:

- [Documentation Index](docs/index.md)
- [Evaluator Path](docs/guides/evaluator-path.md)
- [Architecture Overview](docs/architecture.md)
- [Quick Start](docs/quick-start.md)
- [Local Smoke Test](docs/guides/local-smoke-test.md)
- [CLI Reference](docs/cli.md)
- [Configuration](docs/configuration.md)
- [Troubleshooting](docs/troubleshooting.md)
- [CI and Test Strategy](docs/guides/ci-strategy.md)
- [Public Readiness Checklist](docs/guides/public-readiness.md)

Policies:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)
- [SECURITY.md](SECURITY.md)
- [CHANGELOG.md](CHANGELOG.md)

## Repository Runtime State

This repository intentionally keeps a small sanitized `.ralph/` state for reproducible examples and documentation.
In most consumer repositories, `.ralph/` is project-local runtime state managed by `ralph init`.

## Development

```bash
# Required everyday gate
make agent-ci

# Heaviest final gate before release/publication
make release-gate

# Public-readiness audit
make pre-public-check
```

`make agent-ci` is the command most contributors and agents should use by default. The lower-level targets (`ci-docs`, `ci-fast`, `ci`, `macos-ci`) still exist, but they are mainly the implementation details behind that router and explicit power-user escape hatches.

## License

MIT
