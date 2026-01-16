# Implementation Queue

## Queue
- [ ] RQ-0001 [code]: Sample queue item for fixtures. (ralph_tui/internal/pin/pin.go)
  - Evidence: Fixture coverage requires a valid queue item.
  - Plan: Keep the fixture in sync with queue validation rules.
- [ ] RQ-0006 [code]: Sample queue item with optional Notes. (ralph_tui/internal/pin/pin.go)
  - Evidence: Items can have optional Notes metadata for additional context.
  - Plan: Demonstrate Notes bullet support in fixtures.
  - Notes: This is optional extra context that won't break validation.
    - Link: https://example.com/spec
- [x] RQ-0002 [ops]: Completed queue item for fixtures. (ralph_tui/internal/pin/pin.go)
  - Evidence: Checked items should move into the Done log.
  - Plan: Verify MoveCheckedToDone inserts this block.

## Blocked
- [ ] RQ-0003 [ops]: Blocked fixture item. (README.md)
  - Blocked reason: waiting on fixture update
  - WIP branch: ralph/wip/RQ-0003/20260101_000000
  - Known-good: deadbeef
  - Unblock hint: refresh fixture data

## Parking Lot
- [ ] RQ-0004 [docs]: Parked fixture item. (README.md)
  - Evidence: Placeholder parked item.
  - Plan: Keep parking lot examples for validation.
