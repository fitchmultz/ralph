# Security Model
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](index.md)


Purpose: describe Ralph’s trust boundaries, secret-handling expectations, and publication safety controls.

## Threat Model Scope

In scope:

- Accidental secret leakage into tracked files
- Accidental commit of runtime artifacts with sensitive context
- Misleading publication state caused by stale docs/gates

Out of scope:

- Compromise of third-party runner providers
- Host OS or endpoint malware compromise

## Trust Boundaries

1. Local repo + `.ralph/` state
   - Trusted for local persistence
   - Must remain schema-valid and auditable

2. Runner CLIs and external APIs
   - Potentially untrusted transport boundary
   - Task/context data may be transmitted externally depending on runner setup

3. Build/release tooling
   - Must fail closed on tracked env/runtime artifacts

## Secret and Config Handling

- Never commit `.env` or `.env.*` (except `.env.example`)
- Keep runtime directories local-only:
  - `.ralph/cache/`
  - `.ralph/logs/`
  - `.ralph/lock/`
  - `.ralph/workspaces/`
  - `.ralph/undo/`
  - `.ralph/webhooks/`
- Treat `.ralph/config.jsonc` as sensitive unless intentionally sanitized

## Guardrails

- `.gitignore` blocks runtime artifacts by default
- `check-repo-safety` (via Makefile) runs publication safety checks in day-to-day gates
- `scripts/pre-public-check.sh` enforces:
  - required public-facing files
  - tracked artifact detection
  - tracked env-file detection
  - repo-wide working-tree high-confidence secret-pattern scan with explicit allowlisting
  - repo-wide working-tree markdown link scan
  - documented session-cache path validation
  - optional shared release gate (`make release-gate`)

## Operator Checklist

Before publication:

```bash
make agent-ci
make release-gate
make pre-public-check
```

If any step fails, treat as release blocker.
