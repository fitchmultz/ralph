# Ralph Roadmap

Last updated: 2026-03-13

This is the canonical near-term roadmap for active follow-up work.

## Active roadmap

### 1. Triage the known unrelated `make agent-ci` Rust failures

Why this is the only remaining active item:
- The macOS Settings stabilization track is complete:
  - deterministic Settings smoke coverage now guards the first-open helper-window regression
  - supported Settings entry paths are validated through the same smoke flow
  - `make macos-ci` was run and only the pre-existing unrelated Rust CI failures remain
- Keeping only the remaining Rust work here avoids reopening the completed Settings cutover.

Known failures to address in this follow-up track:
- `ci_gate_compliance_message_passed_to_runner_on_failure`
- `ci_gate_custom_command_shown_in_compliance_message`
- `ci_gate_repeated_same_error_escalates_before_retry_limit`

## Sequencing rules

- Do not reopen the completed Settings stabilization track unless a new regression appears.
- Keep the remaining work focused on the unrelated Rust CI failures until they are resolved.
