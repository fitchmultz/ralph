# Implementation Queue

## Queue
- [ ] RQ-0464 [code][ui][docs]: Add configurable project type (default: code; option: docs) to drive prompts and workflows for non-code repos. (ralph_tui/internal/config, ralph_tui/internal/tui/config_editor.go, ralph_tui/internal/prompts/defaults/, ralph_tui/internal/specs, ralph_tui/internal/loop, ralph_tui/internal/pin, README.md)
  - Evidence: Current prompt templates assume code-heavy repos, which performs poorly on doc-heavy knowledge bases; users need a docs-focused flow for documentation improvements, link fixes, and research synthesis.
  - Plan: Add `project_type` config (default `code`, allow `docs`) and persist it in pin/config; surface it in the config editor; during `ralph init`, prompt for repo type (with optional auto-detect + confirmation) so new repos start with the right prompts; select prompt templates for specs and loop runs based on project type; add docs-focused prompt variants (doc maintenance, link checks, research synthesis) and tests to ensure prompt selection + config round-trips per type.

## Blocked

## Parking Lot
