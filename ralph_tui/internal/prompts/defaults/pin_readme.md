# Ralph Pin Files

These pin files drive the Ralph TUI/CLI workflow. In a fresh repo, run:

  ralph init

The pin directory should include:
- implementation_queue.md
- implementation_done.md
- lookup_table.md
- specs_builder.md
- specs_builder_docs.md (docs projects)

## Queue IDs
To allocate the next queue ID, run:

  ralph pin next-id

This scans the queue and done logs and prints the next available `RQ-####`.

If duplicates slip in, repair them with:

  ralph pin fix-ids

## Queue item metadata
Queue items require `Evidence` and `Plan` bullets. You may add extra metadata after those bullets using
indented notes/links or an indented YAML block. Keep extra metadata indented by two spaces so it stays
inside the queue item block.

Example:

  - [ ] RQ-1234 [code]: Add richer queue metadata support. (ralph_tui/internal/pin/pin.go)
    - Evidence: Users want extra context without breaking parsing.
    - Plan: Support indented Notes/Links and a YAML metadata block.
    - Notes: Optional extra context.
      - Link: https://example.com/spec
    ```yaml
    owner: ralph-team
    severity: medium
    links:
      - https://example.com/spec
    ```
