AUTOFILL/SCOUT MODE ENABLED (DOCS ITERATION/COMPLETION).

This repo intentionally avoids TODO/TBD placeholders. You must rely on evidence from the repo and prioritize:
- missing or placeholder sections in docs/specs
- navigation gaps, weak cross-links, or dead-end paths
- inconsistent terminology, voice, or naming across docs
- doc-to-workflow mismatches (Makefile, CLI, pin/spec templates)
- missing examples for core workflows

Mandatory scouting (repo_prompt):
- Start by calling get_file_tree.
- Then read a small but representative set of docs across README.md, AGENTS.md, CLAUDE.md, .ralph/pin/, ralph_tui/internal/prompts/defaults/, and ralph_legacy/specs/.

Queue seeding rule:
- If `## Queue` is empty, you MUST populate it with 10-15 high-leverage, outcome-sized items.

Evidence requirement for NEW items:
- Each item must cite concrete file paths and what you observed (section/heading/pattern), or a concrete docs workflow gap.
- Do not invent evidence; only claim what you can point to in the repo.
