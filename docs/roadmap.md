# Ralph Roadmap

Last updated: 2026-03-13

This is the canonical near-term roadmap for active follow-up work. It is ordered to minimize churn: protect the landed fix with the narrowest follow-up first, verify supported paths next, run the broader app gate after that, and defer unrelated CI work until the Settings track is fully settled.

## Active roadmap

### 1. Add focused regression coverage for Settings presentation

Why first:
- The macOS Settings helper-window bug was timing-sensitive and AppKit-driven.
- The next highest-value step is a narrow regression check that protects the newly landed first-responder fix from recurrence.

Scope:
- Add a macOS app-level regression test or deterministic smoke path for first-open Settings behavior.
- Assert the Settings window opens without reentering the initial text-field / field-editor path that previously spawned helper windows.
- Prefer validating the invariant through supported window/focus behavior rather than brittle private AppKit class-name matching.

### 2. Validate every supported Settings entry path against the same invariant

Why second:
- `Cmd+,` was the primary repro path, but the app also has command-surface Settings presentation paths.
- Once regression coverage exists, every supported entry path should be checked against the same clean-open invariant.

Scope:
- Validate `Cmd+,`
- Validate app-menu / command-surface Settings open flow
- Validate any menu-bar or URL-routed Settings entry points that remain supported
- Confirm workspace retargeting does not rebuild Settings into the bad initial-focus state

### 3. Run the macOS gate for the finalized Settings cutover

Why third:
- After regression coverage and path validation are in place, the next step is full app-level verification.
- This is the first repository-scale confirmation that the Settings fix does not regress adjacent macOS behavior.

Scope:
- Run `make macos-ci`
- Fix any app-test regressions before reopening unrelated workstreams

### 4. Triage the known unrelated `make agent-ci` Rust failures

Why last:
- These failures predate the Settings fix and are not part of the root-cause or validation loop for the macOS window artifact issue.
- Keeping them last avoids mixing an app-level stabilization track with unrelated Rust CI work.

Known failures to address in that follow-up track:
- `ci_gate_compliance_message_passed_to_runner_on_failure`
- `ci_gate_custom_command_shown_in_compliance_message`
- `ci_gate_repeated_same_error_escalates_before_retry_limit`

## Sequencing rules

- Do not reopen broad Settings refactors before the current first-responder fix has regression coverage and path validation.
- Do not mix unrelated Rust CI fixes into the Settings stabilization track.
- Prefer one stabilization step at a time: regression coverage -> path validation -> full macOS gate -> unrelated CI triage.
