# Project Operating Constitution

Status: Active
Owner: Maintainers
Source of truth: this document
Parent: [Ralph Documentation](../index.md)
Related: [Decisions](../decisions.md), [CI and Test Strategy](ci-strategy.md)
Last updated: 2026-04-23

This constitution defines how Ralph project work is accepted, modified, and
closed. It is intentionally compact: use it to prevent drift, not to create a
second project-management system.

## Core Rules

1. **Simplicity is the default.** Solve the real requirement with the simplest
   complete design. Do not add abstraction, options, layers, or future-proofing
   without a current proven constraint.
2. **One canonical path per thing.** Every recurring action must have exactly
   one approved workflow, one owner, and one obvious entry point. Delete or
   archive old paths when a new path replaces them.
3. **Single source of truth.** Every important fact, config, parameter,
   requirement, decision, status, risk, validation plan, release plan, and
   post-change record must have one authoritative home. Copies must be marked
   as derived and point back to the source.
4. **Centralize configs and assumptions.** No hidden knobs, scattered
   parameters, magic numbers, tribal knowledge, or undocumented side effects.
5. **Full cutover when functionality changes.** Replacements must remove or
   disable the old path unless a real external obligation requires a controlled
   migration window with owner, end date, and removal task.
6. **Clarity before power.** Naming, structure, and user-facing flows must be
   obvious before they become flexible or powerful.
7. **Organization is part of the product.** Every important artifact needs an
   obvious home, current version, parent link, and related links. Do not rely on
   search as the primary navigation method.
8. **No patchwork fixes.** Identify the root cause before accepting a fix.
   Temporary workarounds require owner, reason, risk, expiration date, cleanup
   task, and verification method.
9. **Reuse without abstraction theater.** Reuse stable patterns when they reduce
   total complexity. Do not create generic frameworks for one-off needs.
10. **Test meaningful risks.** Prioritize core user paths, data loss,
    destructive actions, integration points, historical failure modes,
    high-frequency operations, and consequential edge cases.
11. **Safeguards stay on by default.** Destructive or risky actions need visible
    previews, confirmations, validation, and recovery paths where feasible.
12. **Documentation is navigable infrastructure.** Keep current instructions,
    historical records, and decisions separate. Archive or delete stale docs.
13. **Downstream dependents stay synchronized.** A change is incomplete until
    affected docs, configs, tools, tests, apps, scripts, agents, and users are
    updated or explicitly marked unaffected.
14. **Human UX comes first.** A workflow is not done until a human can see
    current state, understand the next action, catch mistakes before
    consequences, and recover from common failures.
15. **Ownership, status, and meaning must be explicit.** Execution requires
    defined owners, decision-makers, acceptance criteria, status labels,
    handoff receivers, dates, and exceptions.
16. **Define success before execution.** Write the desired outcome, acceptance
    criteria, non-goals, constraints, stakeholders, and failure conditions before
    making substantial changes.
17. **Control scope actively.** Classify new work as in-scope, out-of-scope, or
    future. Do not allow silent scope expansion.
18. **Record important decisions.** Use the canonical decision log:
    [docs/decisions.md](../decisions.md).
19. **Prefer deletion over accumulation.** Remove obsolete files, workflows,
    references, tools, branches, plans, and experiments from active areas.
20. **Govern every exception.** Exceptions must be explicit, justified, owned,
    temporary unless formally adopted, and visible to downstream users.
21. **Optimize for maintainability.** The project must remain understandable and
    operable after the original creator is gone.
22. **Make state visible.** Current, broken, pending, approved, deprecated,
    archived, rejected, complete, and blocked states must be visible.
23. **Build feedback loops.** Recurring drift, exceptions, or user friction
    trigger structural correction, not another workaround.
24. **Minimize handoffs.** When handoffs are necessary, define sender, receiver,
    inputs, outputs, acceptance criteria, and source-of-truth artifact.
25. **Use standards and conventions.** Repeatable work should use repeatable
    naming, folder, document, status, decision, and validation formats.

## Required Sources of Truth

- Project overview and navigation: [docs/index.md](../index.md)
- Product overview and first workflow: [README.md](../../README.md)
- Architecture: [docs/architecture.md](../architecture.md)
- Requirements and feature specs: `docs/prd/`
- Decisions: [docs/decisions.md](../decisions.md)
- Active roadmap: [docs/roadmap.md](../roadmap.md)
- Configuration: [docs/configuration.md](../configuration.md)
- CLI reference: [docs/cli.md](../cli.md)
- Machine/app contract: [docs/machine-contract.md](../machine-contract.md)
- CLI/app parity registry: [`crates/ralph/src/cli/app_parity.rs`](../../crates/ralph/src/cli/app_parity.rs)
- Validation gates: [docs/guides/ci-strategy.md](ci-strategy.md)
- Release and cutover: [docs/guides/release-runbook.md](release-runbook.md)
- Public readiness: [docs/guides/public-readiness.md](public-readiness.md)
- Agent-only repository guidance: [AGENTS.md](../../AGENTS.md)

## Pre-Change Checklist

- What is the source of truth?
- What is the canonical path?
- What real problem is being solved?
- What is explicitly out of scope?
- What will be removed, not just added?
- What downstream items depend on this?
- What users or stakeholders are affected?
- What safety checks must remain?
- What validation proves this worked?
- What documentation must change?
- What would make this a hack instead of a clean fix?

## Definition of Done

Work is done only when:

- The actual requirement is satisfied by the simplest viable complete solution.
- There is one canonical path and one source of truth.
- Configs, parameters, assumptions, ownership, status, risks, and exceptions are
  explicit.
- Obsolete paths are removed or archived.
- No unmanaged workaround remains.
- Downstream dependents are updated or explicitly marked unaffected.
- Documentation and decision records are updated in the right canonical homes.
- Meaningful validation is complete.
- Human UX has been checked when a human-facing surface changed.
- No unresolved ambiguity blocks correct use.

## Drift Detection

Stop feature work and run a cleanup pass when three or more are true:

- There are multiple ways to do the same thing.
- There are multiple places to find the same answer.
- People or agents rely on memory or chat history.
- Old workflows are still visible in active areas.
- Configs or parameters are scattered.
- Temporary exceptions have no owner or expiration date.
- Downstream dependents are out of sync.
- Docs are hard to navigate.
- Users are confused about what to do next.
- Local fixes are replacing structural fixes.
- Tests do not map to real risk.
- The project is accumulating instead of simplifying.

## Cleanup Pass

1. Identify the current source of truth.
2. Freeze non-urgent additions.
3. Inventory active artifacts.
4. Identify duplicates and stale items.
5. Pick the canonical path.
6. Delete or archive obsolete material.
7. Centralize parameters and decisions.
8. Rebuild navigation links.
9. Update downstream dependents.
10. Validate the core user path.
11. Record what changed.

## Red-Flag Phrases

Treat these as warnings that require clarification or cleanup:

- "For now"
- "Just add another"
- "Both ways work"
- "It depends who you ask"
- "Check the chat"
- "This is mostly done"
- "We'll clean it up later"
- "Don't touch that"
- "Only one person understands it"
- "It was easier to patch"
- "The old one still works"
- "Users will figure it out"
- "We don't need to document this"
