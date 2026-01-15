# Implementation Queue

## Queue
- [ ] RQ-0413 [code]: Unify reasoning_effort + runner args handling across TUI and loop runner; make "auto" semantics consistent. (ralph_tui/internal/tui/args_helpers.go, ralph_tui/internal/tui/specs_view.go, ralph_tui/internal/tui/loop_view.go, ralph_tui/internal/loop/loop.go)
  - Evidence: there are two separate effort detection/injection implementations (tui/args_helpers.go and loop/loop.go containsEffort/detectEffort); specsView currently maps "auto" to a hard-coded default ("medium") and injects -c even when the user expects no override, while the loop runner injects its own effort based on [P1], creating confusing mismatches.
  - Plan: Centralize effort parsing/injection into a shared helper and define explicit semantics for auto/off; ensure specs/loop both use the same logic and the UI shows the effective effort being applied; add tests for codex args injection precedence and "auto" behavior.
- [ ] RQ-0414 [code]: Fix CLI subcommands ignoring config defaults (runner, runner_args, reasoning_effort), making CLI runs diverge from the TUI. (ralph_tui/cmd/ralph/main.go, ralph_tui/internal/config/load.go, ralph_tui/internal/tui/args_helpers.go)
  - Evidence: `ralph loop run` defaults runnerName to "codex" and uses only positional args, ignoring cfg.Loop.Runner/RunnerArgs; `ralph specs build` defaults to codex and ignores cfg.Specs.Runner/RunnerArgs/ReasoningEffort unless flags are explicitly provided. This forces users to re-specify settings and causes surprising CLI-vs-TUI behavior differences.
  - Plan: When flags are not changed, default to cfg.Specs/Loop values; merge config runner args with CLI args; apply reasoning_effort consistently via shared helper; add unit tests for precedence (config vs flags vs positionals).
- [ ] RQ-0415 [code]: Remove or implement dead config knobs (runner.max_workers/dry_run, loop.workers/poll_seconds, git.require_clean/commit_prefix) so settings actually do something. (ralph_tui/internal/config/config.go, ralph_tui/internal/config/defaults.json, ralph_tui/internal/tui/config_editor.go, ralph_tui/cmd/ralph/main.go, ralph_tui/internal/loop/loop.go)
  - Evidence: config schema + config editor expose several fields that are not wired into behavior anywhere (runner.max_workers, runner.dry_run, loop.workers, loop.poll_seconds, git.require_clean, git.commit_prefix), so users can change them but nothing changes at runtime.
  - Plan: Audit each knob and either wire it into loop/specs/TUI behavior with clear semantics, or deprecate/remove it with a migration step and updated docs/tests; ensure config validation stays correct and does not enforce unused fields.
- [ ] RQ-0416 [code]: Consolidate specs builder logic so Go TUI/CLI and legacy script do not drift. (ralph_tui/internal/specs/specs.go, ralph_legacy/bin/build_specs.sh)
  - Evidence: `ralph_tui/internal/specs/specs.go` implements locking, prompt template validation, runner selection, and autofill-scout logic; `ralph_legacy/bin/build_specs.sh` re-implements the same flow in shell, including separate defaults and lock handling, which risks divergence.
  - Plan: Choose a single canonical implementation (likely `ralph specs build` in Go) and have the legacy script delegate to it or deprecate/remove the script; align defaults (prompt template path, autofill-scout policy, runner checks); update docs/tests to cover the canonical path.
- [ ] RQ-0417 [docs]: Standardize specs template location and references across legacy + TUI to avoid split-brain prompts. (ralph_tui/internal/specs/specs.go, ralph_legacy/specs/specs_builder.md, .ralph/pin/specs_builder.md)
  - Evidence: Go specs builder defaults to `.ralph/pin/specs_builder.md`, while `ralph_legacy/bin/build_specs.sh` defaults to `ralph_legacy/specs/specs_builder.md`, creating two separate templates with overlapping instructions.
  - Plan: Decide on a single template source of truth (pin vs legacy), migrate the other path to a redirect or wrapper, and update any docs or scripts referencing the non-canonical location.

## Blocked

## Parking Lot
