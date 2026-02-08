# macOS App

Purpose: Document Ralph's macOS SwiftUI app and how it fits into the overall workflow.

## Overview

Ralph includes a macOS app (SwiftUI) for interactive work with your task queue.

The app is intended for:
- Browsing and editing `.ralph/queue.json` and `.ralph/done.json`
- Creating and triaging tasks during day-to-day development
- Opening a repo in a dedicated UI without leaving the CLI workflow

The app does not:
- Replace the CLI for automation, scripting, or CI usage
- Run on non-macOS platforms

## Open The App

From a repository that has been initialized with `ralph init`:

```bash
ralph app open
```

If you are not on macOS (or you prefer staying in the terminal), use the CLI:

```bash
ralph queue list
ralph run one
ralph run loop
```

## Notes

- The app expects the repo-local `.ralph/` files to exist.
- For advanced operations and full flag coverage, use the CLI reference: `docs/cli.md`.

