# Implementation Queue

## Queue
- [ ] RQ-0496 [code]: Change default ralph data directory to PWD .ralph/data instead of PWD data/. (ralph_tui/internal/config, ralph_tui/cmd/ralph/main.go)
  - Evidence: Current default data directory is `data/` at project root; should be `.ralph/data/` to keep all ralph-related files under `.ralph/`.
  - Plan: Update default data directory path from `data/` to `.ralph/data/` in configuration; update any path references and tests; ensure directory creation logic handles the new path correctly.
- [ ] RQ-0497 [code]: Change default log path to PWD .ralph/logs/ralph.log. (ralph_tui/internal/config, ralph_tui/cmd/ralph/main.go)
  - Evidence: Current default log path needs to be updated to `.ralph/logs/ralph.log` to keep all ralph-related files under `.ralph/`.
  - Plan: Update default log path to `.ralph/logs/ralph.log` in configuration; ensure logs directory is created if needed; update any path references and tests.


## Blocked

## Parking Lot
