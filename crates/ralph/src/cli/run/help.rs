//! Long-help text for `ralph run`.
//!
//! Purpose:
//! - Long-help text for `ralph run`.
//!
//! Responsibilities:
//! - Centralize verbose clap help text separately from clap type definitions.
//!
//! Not handled here:
//! - CLI argument parsing or dispatch.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Help strings stay as `'static` constants for clap attributes.

pub(super) const RUN_AFTER_LONG_HELP: &str = "Runner selection:\n\
  - `ralph run` selects runner/model/effort with this precedence:\n\
  1) CLI overrides (flags on `run one` / `run loop`)\n\
  2) task's `agent` override (runner/model plus `model_effort` if set)\n\
  3) otherwise: resolved config defaults (`agent.runner`, `agent.model`, `agent.reasoning_effort`).\n\
 \n\
 Resume behavior:\n\
  - Ralph now narrates whether it is resuming the same session, starting fresh, or refusing to guess.\n\
  - `ralph run one` inspects interrupted-session state too; add `--resume` to auto-continue when safe.\n\
  - Timed-out sessions still require explicit confirmation and are refused in non-interactive mode.\n\
 \n\
 Blocking-state diagnosis:\n\
  - Ralph uses one canonical BlockingState vocabulary everywhere: waiting, blocked, stalled.\n\
  - Recovery-entry commands (`task mutate`, `task decompose`, `queue validate`, `queue repair`, `undo`) use the same waiting/blocked/stalled narration when Ralph needs operator guidance.\n\
  - Canonical reasons are: idle, dependency_blocked, schedule_blocked, lock_blocked, ci_blocked, runner_recovery, operator_recovery, mixed_queue.\n\
  - Use `ralph doctor` for human-readable diagnosis when Ralph is not making progress.\n\
  - Use `ralph doctor --format json` or `ralph machine doctor report` for machine-readable blocking diagnosis.\n\
 \n\
 Notes:\n\
  - Allowed runners: codex, opencode, gemini, claude, cursor, kimi, pi\n\
  - Allowed models: gpt-5.4, gpt-5.3-codex, gpt-5.3-codex-spark, gpt-5.3, zai-coding-plan/glm-4.7, gemini-3-pro-preview, gemini-3-flash-preview, sonnet, opus, kimi-for-coding (codex supports only gpt-5.4 + gpt-5.3-codex + gpt-5.3-codex-spark + gpt-5.3; opencode/gemini/claude/cursor/kimi/pi accept arbitrary model ids)\n\
  - `--effort` is codex-only and is ignored for other runners.\n\
  - `--git-revert-mode` controls whether Ralph reverts uncommitted changes on errors (ask, enabled, disabled).\n\
  - `--git-publish-mode` controls post-run git behavior: off, commit, or commit_and_push.\n\
  - `--parallel` is experimental and runs loop tasks concurrently in workspaces (clone-based).\n\
  - Experimental parallel workers push directly to the target branch after phase execution.\n\
  - Clean-repo checks allow changes to `.ralph/config.jsonc`, `.ralph/queue.jsonc`, and `.ralph/done.jsonc`; use `--force` to bypass entirely.\n\
 \n\
Phase-specific overrides:\n\
  Use --runner-phaseN, --model-phaseN, --effort-phaseN to override settings for a specific phase.\n\
  Phase-specific flags take precedence over global flags for that phase.\n\
  Single-pass (--phases 1) uses Phase 2 overrides.\n\
 \n\
  Precedence per phase (highest to lowest):\n\
    1) CLI phase override (--runner-phaseN, --model-phaseN, --effort-phaseN)\n\
    2) Task phase override (task.agent.phase_overrides.phaseN.*)\n\
    3) Config phase override (agent.phase_overrides.phaseN.*)\n\
    4) CLI global override (--runner, --model, --effort)\n\
    5) Task global override (task.agent.runner/model/model_effort)\n\
    6) Config defaults (agent.*)\n\
 \n\
 To change defaults for this repo, edit .ralph/config.jsonc:\n\
  version: 2\n\
  agent:\n\
  runner: codex\n\
  model: gpt-5.4\n\
  gemini_bin: gemini\n\
 \n\
Examples:\n\
 ralph run one\n\
 ralph run one --resume\n\
 ralph run one --phases 2\n\
 ralph run one --phases 1\n\
 ralph run one --runner opencode --model gpt-5.3\n\
 ralph run one --runner codex --model gpt-5.4 --effort high\n\
 ralph run one --runner-phase1 codex --model-phase1 gpt-5.4 --effort-phase1 high\n\
 ralph run one --runner-phase2 claude --model-phase2 opus\n\
 ralph run one --runner gemini --model gemini-3-flash-preview\n\
 ralph run one --runner pi --model gpt-5.3\n\
 ralph run one --include-draft\n\
 ralph run one --git-revert-mode disabled\n\
 ralph run one --git-publish-mode off\n\
 ralph run one --lfs-check\n\
 ralph run loop --max-tasks 0\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.3\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --git-revert-mode ask --max-tasks 1\n\
 ralph run loop --git-publish-mode commit_and_push --max-tasks 1\n\
 ralph run loop --lfs-check --max-tasks 1\n\
 ralph run loop --parallel --max-tasks 4\n\
 ralph run loop --parallel 4 --max-tasks 8\n\
 ralph run resume\n\
 ralph run resume --force\n\
 ralph run loop --resume --max-tasks 5";

pub(super) const RESUME_AFTER_LONG_HELP: &str = "Resume behavior:\n\
 - If the saved session is still valid, Ralph resumes the same interrupted task.\n\
 - If the saved session is stale or no longer safe, Ralph says so and starts fresh.\n\
 - If confirmation is required but unavailable (for example `--non-interactive`), Ralph refuses instead of guessing.\n\
\n\
Examples:\n\
 ralph run resume\n\
 ralph run resume --force\n\
 ralph run resume --non-interactive";

pub(super) const RUN_ONE_AFTER_LONG_HELP: &str = "Runner selection (precedence):\n\
 1) CLI overrides (--runner/--model/--effort)\n\
 2) task.agent in the configured queue file (if present)\n\
 3) selected profile (if --profile specified)\n\
 4) config defaults (.ralph/config.jsonc then ~/.config/ralph/config.jsonc)\n\
\n\
Resume behavior:\n\
 - `ralph run one` inspects interrupted-session state before selecting work.\n\
 - `ralph run one --resume` auto-resumes the interrupted session when Ralph can do so safely.\n\
 - Explicit `--id <TASK_ID>` beats an unrelated interrupted session, and Ralph says so.\n\
 - If confirmation is required but unavailable (for example `--non-interactive`), Ralph refuses instead of silently guessing.\n\
\n\
Blocking-state diagnosis:\n\
 - If Ralph refuses to continue, stalls, or appears blocked, run `ralph doctor`.\n\
 - `ralph doctor` uses the same BlockingState explanation shown by run, machine, and app surfaces.\n\
\n\
Examples:\n\
 ralph run one\n\
 ralph run one --resume\n\
 ralph run one --id RQ-0001\n\
 ralph run one --id RQ-0001 --resume\n\
 ralph run one --debug\n\
 ralph run one --profile fast-local\n\
 ralph run one --profile deep-review\n\
 ralph run one --phases 3 (plan/implement+CI/review+complete)\n\
 ralph run one --phases 2 (plan/implement)\n\
 ralph run one --phases 1 (single-pass)\n\
 ralph run one --quick (single-pass, same as --phases 1)\n\
 ralph run one --runner opencode --model gpt-5.3\n\
 ralph run one --runner gemini --model gemini-3-flash-preview\n\
 ralph run one --runner pi --model gpt-5.3\n\
 ralph run one --runner codex --model gpt-5.4 --effort high\n\
 ralph run one --runner-phase1 codex --model-phase1 gpt-5.4 --effort-phase1 high\n\
 ralph run one --runner-phase2 claude --model-phase2 opus\n\
 ralph run one --include-draft\n\
 ralph run one --git-revert-mode enabled\n\
 ralph run one --git-publish-mode off\n\
 ralph run one --lfs-check\n\
 ralph run one --repo-prompt plan\n\
 ralph run one --repo-prompt off\n\
 ralph run one --non-interactive\n\
 ralph run one --dry-run\n\
 ralph run one --dry-run --include-draft\n\
 ralph run one --dry-run --id RQ-0001";

pub(super) const RUN_LOOP_AFTER_LONG_HELP: &str = "Resume behavior:\n\
 - `ralph run loop --resume` auto-resumes the interrupted session when safe.\n\
 - Without `--resume`, Ralph still narrates stale/fresh/refusal cases instead of hiding them.\n\
 - If confirmation is required but unavailable (for example `--non-interactive`), Ralph refuses instead of silently guessing.\n\
\n\
Blocking-state diagnosis:\n\
 - `ralph run loop` emits canonical blocking states during waiting and stall transitions.\n\
 - Use `ralph doctor` to diagnose the same state outside the live run loop.\n\
\n\
Examples:\n\
 ralph run loop --max-tasks 0\n\
 ralph run loop --profile fast-local --max-tasks 5\n\
 ralph run loop --profile deep-review --max-tasks 5\n\
 ralph run loop --resume --max-tasks 5\n\
 ralph run loop --phases 3 --max-tasks 0 (plan/implement+CI/review+complete)\n\
 ralph run loop --phases 2 --max-tasks 0 (plan/implement)\n\
 ralph run loop --phases 1 --max-tasks 1 (single-pass)\n\
 ralph run loop --quick --max-tasks 1 (single-pass, same as --phases 1)\n\
 ralph run loop --max-tasks 3\n\
 ralph run loop --max-tasks 1 --debug\n\
 ralph run loop --max-tasks 1 --runner opencode --model gpt-5.3\n\
 ralph run loop --runner-phase1 codex --model-phase1 gpt-5.4 --effort-phase1 high --max-tasks 1\n\
 ralph run loop --runner-phase2 claude --model-phase2 opus --max-tasks 1\n\
 ralph run loop --include-draft --max-tasks 1\n\
 ralph run loop --git-revert-mode disabled --max-tasks 1\n\
 ralph run loop --git-publish-mode off --max-tasks 1\n\
 ralph run loop --repo-prompt tools --max-tasks 1\n\
 ralph run loop --repo-prompt off --max-tasks 1\n\
 ralph run loop --lfs-check --max-tasks 1\n\
 ralph run loop --dry-run\n\
 ralph run loop --wait-when-blocked\n\
 ralph run loop --wait-when-blocked --wait-timeout-seconds 600\n\
 ralph run loop --wait-when-blocked --wait-poll-ms 250\n\
 ralph run loop --wait-when-blocked --notify-when-unblocked";

pub(super) const PARALLEL_AFTER_LONG_HELP: &str = "Experimental direct-push parallel execution.\n\
\n\
Examples:\n\
 ralph run parallel status\n\
 ralph run parallel status --json\n\
 ralph run parallel retry --task RQ-0001";

pub(super) const PARALLEL_STATUS_AFTER_LONG_HELP: &str = "Examples:\n\
 ralph run parallel status\n\
 ralph run parallel status --json\n\
 ralph run parallel retry --task RQ-0001";

pub(super) const PARALLEL_RETRY_AFTER_LONG_HELP: &str = "Examples:\n\
 ralph run parallel retry --task RQ-0001\n\
 ralph run loop --parallel <N>";
