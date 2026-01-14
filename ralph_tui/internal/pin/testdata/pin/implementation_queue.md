# Implementation Queue

## Queue
- [ ] IDFQ-0001 [code]: Sample queue item for fixtures. (ralph_tui/internal/pin/pin.go)
  - Evidence: Fixture coverage requires a valid queue item.
  - Plan: Keep the fixture in sync with queue validation rules.
- [x] IDFQ-0002 [ops]: Completed queue item for fixtures. (ralph_tui/internal/pin/pin.go)
  - Evidence: Checked items should move into the Done log.
  - Plan: Verify MoveCheckedToDone inserts this block.

## Blocked
- [ ] IDFQ-0003 [ops]: Blocked fixture item. (README.md)
  - Blocked reason: waiting on fixture update
  - WIP branch: ralph/wip/IDFQ-0003/20260101_000000
  - Known-good: deadbeef
  - Unblock hint: refresh fixture data

## Parking Lot
- [ ] IDFQ-0004 [docs]: Parked fixture item. (README.md)
  - Evidence: Placeholder parked item.
  - Plan: Keep parking lot examples for validation.
