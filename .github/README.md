# .github

Purpose: GitHub-hosted repository metadata, templates, and narrowly scoped workflow glue.

## What lives here
- `ISSUE_TEMPLATE/` — issue intake templates and config
- `PULL_REQUEST_TEMPLATE.md` — PR author guidance
- `release-notes-template.md` — release note scaffolding
- `workflows/` — minimal GitHub-hosted automation that complements, but does not replace, local `make agent-ci`

## Current workflow exception
This repo remains **local-CI-first**. The workflow under `workflows/cursor-finish-line-ready.yml` is a demo-only readiness gate that waits for selected Cursor Automation checks and emits a single success check so the separate `PR Finish Line` Cursor automation can sequence after them. It is **not** a substitute for local validation and must stay narrowly scoped.
