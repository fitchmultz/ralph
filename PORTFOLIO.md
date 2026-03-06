# Ralph Portfolio Guide

Purpose: give skeptical reviewers a fast, high-signal validation path.

Ralph is easiest to evaluate as an answer to a simple team problem: "We already use AI coding agents, but we need a local, auditable workflow that turns those one-off chats into queue-driven work with explicit verification." The path below is optimized to prove that quickly.

## Reviewer Personas

- Adopter evaluator: "Can I clone this and get value quickly?"
- Senior engineer reviewer: "Is the architecture intentional, reliable, and maintainable?"
- Security-minded reviewer: "Are defaults safe and publication hygiene enforced?"
- Maintainer/operator: "Are CI gates deterministic and resource-conscious?"

## If You Only Read 3 Things

1. [README.md](README.md) — what Ralph is for, why it matters, and the first end-to-end workflow
2. [docs/guides/reviewer-smoke-test.md](docs/guides/reviewer-smoke-test.md) — deterministic validation path with no external runner setup
3. [docs/architecture.md](docs/architecture.md) — boundaries, flows, and recovery model

## Claim → Evidence

- Claim: local clone to working is straightforward
  - Evidence: [README quick start](README.md#quick-start)
  - Verify: `ralph init && ralph task "smoke" && ralph queue list`

- Claim: PR gate is deterministic and local-first
  - Evidence: [Makefile gates](Makefile), [CI strategy](docs/guides/ci-strategy.md)
  - Verify: `make agent-ci`

- Claim: public-release hygiene is enforced
  - Evidence: [pre-public check script](scripts/pre-public-check.sh), [public checklist](docs/guides/public-readiness.md)
  - Verify: `make pre-public-check`

- Claim: architecture is explainable and recoverable
  - Evidence: [architecture overview](docs/architecture.md)
  - Verify: read trust boundaries + failure/recovery sections

- Claim: security expectations are explicit
  - Evidence: [SECURITY.md](SECURITY.md), [security model](docs/security-model.md)
  - Verify: run secret/runtime checks via `scripts/pre-public-check.sh --skip-ci --skip-links --skip-clean`

## Suggested Reviewer Walkthrough (10 minutes)

```bash
# install from source
make install

# no external runner required for this smoke
ralph init
ralph --help
ralph run one --help
ralph scan --help
ralph queue list
ralph queue graph
ralph queue validate
ralph doctor

# required quality gate
RALPH_CI_JOBS=4 make agent-ci
```

## One Concrete Repo Workflow

Use this when you want to answer "what would adoption look like in a real repo?" without reading deep docs:

```bash
cd your-project
ralph init
ralph task "Add retry coverage for webhook delivery failures"
ralph queue show RQ-0001
ralph run one --phases 3
ralph queue list
ralph doctor
```

What to look for:

- The request becomes explicit queue state in `.ralph/queue.jsonc`.
- The run stays local-first while delegating execution to your configured runner.
- The repo still has an obvious verification path through `ralph doctor` and `make agent-ci`.

## Where the Interesting Engineering Lives

- `crates/ralph/src/main.rs` — startup path and command wiring
- `crates/ralph/src/sanity/mod.rs` — preflight checks and guardrails
- `crates/ralph/src/commands/run/` — supervision, phases, resume/recovery
- `apps/RalphMac/RalphCore/RalphCLIClient.swift` — app ↔ CLI bridge
- `scripts/pre-public-check.sh` — publication safety gates

## Related Evidence

- [Public Readiness Checklist](docs/guides/public-readiness.md)
- [Reviewer Smoke Test](docs/guides/reviewer-smoke-test.md)
- [Role Evidence Index](docs/role-evidence/index.md)
- [Release Readiness Report](docs/guides/release-readiness-report.md)
