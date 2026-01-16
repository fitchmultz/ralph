AUTOFILL/SCOUT MODE ENABLED (BUG-HUNT).

This repo intentionally avoids TODO/TBD placeholders. You must rely on evidence from the repo and prioritize:
- architectural debt and risky coupling
- duplicated logic across packages or layers
- workflow gaps in Makefile or CLI flows
- missing regression tests for brittle paths
- config/state mismatches between defaults, UI, and CLI

Mandatory scouting (repo_prompt):
- Start by calling get_file_tree.
- Then read a small but representative set of files across ralph_tui/internal/, ralph_tui/cmd/, and .ralph/pin/.

Queue seeding rule:
- If `## Queue` is empty, you MUST populate it with 10-15 high-leverage, outcome-sized items.

Evidence requirement for NEW items:
- Each item must cite concrete file paths and what you observed (function/class/pattern), or a concrete Make target/workflow gap.
- Do not invent evidence; only claim what you can point to in the repo.
