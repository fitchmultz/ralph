#!/usr/bin/env bash
# Purpose: Repeatably dogfood Ralph against a disposable git project.
# Responsibilities: Create an isolated fixture repo, exercise Ralph setup/task/queue surfaces, and run one real three-phase agent task.
# Scope: Local dogfood automation only; it does not mutate the Ralph source repo except for writing ignored artifacts under target/.
# Usage: Run from the Ralph repo with `scripts/dogfood-ralph.sh`; use `--help` for options and examples.
# Invariants/Assumptions: Requires git, python3, and a Ralph binary; full Phase 3 requires the configured runner/model to be available.

set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RALPH_BIN="${RALPH_BIN:-$ROOT_DIR/target/debug/ralph}"
OUT_ROOT="$ROOT_DIR/target/dogfood-ralph"
RUNNER="pi"
MODEL="zai-glm-5.1"
MODEL_NOTE=""
PHASES="3"
RUN_REAL_AGENT=1
KEEP_PROJECT=1
PROJECT_NAME="ralph-dogfood-fixture"
GITHUB_PRIVATE=0

usage() {
  cat <<'USAGE'
Repeatably dogfood Ralph against a disposable test project.

Usage:
  scripts/dogfood-ralph.sh [options]

Options:
  --ralph-bin PATH       Ralph binary to test (default: target/debug/ralph or $RALPH_BIN)
  --out-root DIR         Artifact root (default: target/dogfood-ralph)
  --runner NAME          Runner for Phase 3 real execution (default: pi)
  --model ID             Model for Phase 3 real execution (default: zai-glm-5.1;
                         normalized to zai/glm-5.1 for the pi CLI on this machine)
  --phases N             Ralph run phases for Phase 3 (default: 3)
  --skip-real-agent      Run setup/workflow checks but skip `ralph run one`
  --github-private       Create a private GitHub repo for the fixture with gh, then push initial state
  --project-name NAME    Fixture project/repo name (default: ralph-dogfood-fixture)
  -h, --help             Show this help
Examples:
  scripts/dogfood-ralph.sh
  scripts/dogfood-ralph.sh --skip-real-agent
  RALPH_BIN=target/release/ralph scripts/dogfood-ralph.sh --github-private

Exit codes: 0 success; 1 dogfood failure; 2 invalid usage.
USAGE
}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --ralph-bin) RALPH_BIN="$2"; shift 2 ;;
    --out-root) OUT_ROOT="$2"; shift 2 ;;
    --runner) RUNNER="$2"; shift 2 ;;
    --model) MODEL="$2"; shift 2 ;;
    --phases) PHASES="$2"; shift 2 ;;
    --skip-real-agent) RUN_REAL_AGENT=0; shift ;;
    --github-private) GITHUB_PRIVATE=1; shift ;;
    --project-name) PROJECT_NAME="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ ! -x "$RALPH_BIN" ]]; then
  echo "Ralph binary not found or not executable: $RALPH_BIN" >&2
  echo "Build one first, for example: cargo build -p ralph" >&2
  exit 1
fi

if ! command -v git >/dev/null 2>&1; then
  echo "git is required" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required" >&2
  exit 1
fi
if [[ "$GITHUB_PRIVATE" -eq 1 ]] && ! command -v gh >/dev/null 2>&1; then
  echo "gh is required for --github-private" >&2
  exit 1
fi

REQUESTED_MODEL="$MODEL"
if [[ "$RUNNER" == "pi" && "$MODEL" == "zai-glm-5.1" ]]; then
  MODEL="zai/glm-5.1"
  MODEL_NOTE="Requested model zai-glm-5.1 is normalized to pi's available zai/glm-5.1 id."
fi

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="$OUT_ROOT/$STAMP"
PROJECT_DIR="$RUN_DIR/$PROJECT_NAME"
LOG_DIR="$RUN_DIR/logs"
REPORT="$RUN_DIR/report.md"
mkdir -p "$LOG_DIR"

CURRENT_STEP="initializing"
trap 'status=$?; if [[ $status -ne 0 ]]; then echo "\nFAILED during: $CURRENT_STEP" | tee -a "$REPORT" >&2; echo "Artifacts: $RUN_DIR" | tee -a "$REPORT" >&2; fi; exit $status' EXIT

cat >"$REPORT" <<EOF_REPORT
# Ralph Dogfood Report

- Date (UTC): $STAMP
- Ralph binary: $RALPH_BIN
- Fixture project: $PROJECT_DIR
- Runner/model: $RUNNER / $MODEL
- Requested model: $REQUESTED_MODEL
- Model note: ${MODEL_NOTE:-N/A}
- Phases: $PHASES
- Real agent run: $RUN_REAL_AGENT

## Phase Results

EOF_REPORT
log_cmd() {
  local name="$1"
  shift
  local logfile="$LOG_DIR/$name.log"
  CURRENT_STEP="$name"
  set +e
  local status
  {
    echo "# command: $*"
    echo "# cwd: $(pwd)"
    echo "# started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    "$@"
    status=$?
    echo "# exited: $status"
    echo "# ended: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } >"$logfile" 2>&1
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "- PASS $name — logs/$(basename "$logfile")" >>"$REPORT"
  else
    echo "- FAIL $name — exit $status — logs/$(basename "$logfile")" >>"$REPORT"
    return "$status"
  fi
}

log_cmd_allow_fail() {
  local name="$1"
  shift
  local logfile="$LOG_DIR/$name.log"
  CURRENT_STEP="$name"
  set +e
  local status
  {
    echo "# command: $*"
    echo "# cwd: $(pwd)"
    echo "# started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    "$@"
    status=$?
    echo "# exited: $status"
    echo "# ended: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  } >"$logfile" 2>&1
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "- PASS $name — logs/$(basename "$logfile")" >>"$REPORT"
  else
    echo "- FAIL $name — exit $status — logs/$(basename "$logfile")" >>"$REPORT"
  fi
  return "$status"
}
write_fixture() {
  mkdir -p "$PROJECT_DIR/scripts" "$PROJECT_DIR/tests" "$PROJECT_DIR/docs/prd" "$PROJECT_DIR/src"
  cat >"$PROJECT_DIR/README.md" <<'EOF_README'
# Ralph Dogfood Fixture

A tiny Python CLI used to dogfood Ralph. The intended product behavior is:

```bash
python3 greeter.py --name Ada
# Hello, Ada!
```

Run checks with:

```bash
./scripts/ci.sh
```
EOF_README

  cat >"$PROJECT_DIR/AGENTS.md" <<'EOF_AGENTS'
# Fixture Agent Instructions

- Keep changes minimal and user-visible.
- Validate with `./scripts/ci.sh` before completion.
- Do not modify `.ralph/done.jsonc` manually; use Ralph task completion flows.
EOF_AGENTS

  cat >"$PROJECT_DIR/greeter.py" <<'EOF_PY'
#!/usr/bin/env python3
"""Small intentionally incomplete greeting CLI for Ralph dogfood runs."""

import argparse


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Print a greeting.")
    parser.add_argument("--excited", action="store_true", help="Use an exclamation mark.")
    return parser


def greeting(excited: bool = False) -> str:
    punctuation = "!" if excited else "."
    return f"Hello, world{punctuation}"


def main() -> int:
    args = build_parser().parse_args()
    print(greeting(args.excited))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
EOF_PY
  chmod +x "$PROJECT_DIR/greeter.py"

  cat >"$PROJECT_DIR/tests/test_greeter.py" <<'EOF_TEST'
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class GreeterCliTests(unittest.TestCase):
    def run_cli(self, *args: str) -> str:
        completed = subprocess.run(
            [sys.executable, str(ROOT / "greeter.py"), *args],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        return completed.stdout.strip()

    def test_default_greeting(self) -> None:
        self.assertEqual(self.run_cli(), "Hello, world.")

    def test_excited_greeting(self) -> None:
        self.assertEqual(self.run_cli("--excited"), "Hello, world!")


if __name__ == "__main__":
    unittest.main()
EOF_TEST

  cat >"$PROJECT_DIR/scripts/ci.sh" <<'EOF_CI'
#!/usr/bin/env bash
set -euo pipefail
python3 -m unittest discover -s tests -v
python3 greeter.py >/tmp/ralph-dogfood-greeter.out
EOF_CI
  chmod +x "$PROJECT_DIR/scripts/ci.sh"

  cat >"$PROJECT_DIR/docs/prd/named-greeting.md" <<'EOF_PRD'
# Named Greeting Improvements

## Problem

Users need predictable examples for personalized greetings.

## Goals

- Support personalized greetings.
- Keep existing default greeting behavior.

## User Stories

### Story 1: Named greeting
As a CLI user, I can pass a name so that the output greets that person.

Acceptance criteria:
- `--name Ada` prints `Hello, Ada.`
- `--name Ada --excited` prints `Hello, Ada!`
EOF_PRD

  cat >"$PROJECT_DIR/src/todo_sample.py" <<'EOF_TODO'
# TODO: add localization support after named greetings are stable.
EOF_TODO
}

seed_task() {
  python3 - "$PROJECT_DIR/.ralph/queue.jsonc" <<'PY'
import json
import sys
from datetime import datetime, timezone

queue_path = sys.argv[1]
now = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
task = {
    "id": "RQ-0001",
    "status": "todo",
    "title": "Add named greeting support to the fixture CLI",
    "description": "Implement a --name option for greeter.py so users can print personalized greetings while preserving the existing default and --excited behavior.",
    "priority": "high",
    "tags": ["dogfood", "cli", "test-fixture"],
    "scope": ["greeter.py", "tests/test_greeter.py", "README.md", "scripts/ci.sh"],
    "evidence": [
        "README documents `python3 greeter.py --name Ada` as an intended workflow, but the CLI currently has no --name argument.",
        "Existing tests cover only default and --excited greetings; named greeting behavior needs regression coverage."
    ],
    "plan": [
        "Add a --name argument with a sensible default of world.",
        "Update greeting construction so --name and --excited compose correctly.",
        "Add unittest coverage for named greeting and named excited greeting.",
        "Update README only if usage text needs clarification.",
        "Run ./scripts/ci.sh before marking the task complete."
    ],
    "notes": [],
    "request": "Dogfood Ralph by making this fixture CLI support named greetings.",
    "created_at": now,
    "updated_at": now,
    "depends_on": [],
    "blocks": [],
    "relates_to": [],
    "custom_fields": {"dogfood_run": True},
}
with open(queue_path, "w", encoding="utf-8") as fh:
    json.dump({"version": 1, "tasks": [task]}, fh, indent=2)
    fh.write("\n")
PY
}

write_fixture
cd "$PROJECT_DIR"
log_cmd phase0-git-init git init -b main
log_cmd phase0-git-config-name git config user.name "Ralph Dogfood"
log_cmd phase0-git-config-email git config user.email "ralph-dogfood@example.invalid"
log_cmd phase0-initial-ci ./scripts/ci.sh
log_cmd phase0-initial-commit git add README.md AGENTS.md greeter.py scripts/ci.sh tests/test_greeter.py docs/prd/named-greeting.md src/todo_sample.py
log_cmd phase0-git-commit git commit -m "Create Ralph dogfood fixture"

if [[ "$GITHUB_PRIVATE" -eq 1 ]]; then
  log_cmd phase0-gh-create gh repo create "$PROJECT_NAME" --private --source . --remote origin --push
fi

cat >>"$REPORT" <<'EOF_PHASE1'

### Phase 1 — Bootstrap and diagnostics
EOF_PHASE1
log_cmd phase1-ralph-init "$RALPH_BIN" init --non-interactive --no-color

python3 - "$PROJECT_DIR/.ralph/config.jsonc" "$RUNNER" "$MODEL" "$PHASES" <<'PY'
import json
import sys

config_path, runner, model, phases = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
config = {
    "version": 2,
    "agent": {
        "runner": runner,
        "model": model,
        "phases": phases,
        "git_publish_mode": "commit",
        "ci_gate": {"enabled": True, "argv": ["./scripts/ci.sh"]},
        "webhook": {"enabled": False},
        "phase_overrides": {
            "phase1": {"runner": runner, "model": model},
            "phase2": {"runner": runner, "model": model},
            "phase3": {"runner": runner, "model": model},
        },
    },
}
with open(config_path, "w", encoding="utf-8") as fh:
    json.dump(config, fh, indent=2)
    fh.write("\n")
PY
seed_task
log_cmd phase1-config-show "$RALPH_BIN" config show --format json --no-color
log_cmd phase1-doctor "$RALPH_BIN" doctor --no-color
log_cmd phase1-runner-list "$RALPH_BIN" runner list --no-color
log_cmd phase1-prompt-preview "$RALPH_BIN" prompt worker --phase 1 --no-color
log_cmd phase1-version "$RALPH_BIN" version --no-color
log_cmd phase1-cli-spec "$RALPH_BIN" cli-spec --no-color
log_cmd phase1-help-all "$RALPH_BIN" help-all
log_cmd phase1-completions-bash "$RALPH_BIN" completions bash
log_cmd phase1-context-init "$RALPH_BIN" context init --force --project-type python --no-color
log_cmd phase1-top-level-help-matrix bash -c 'set -euo pipefail; for c in init app queue task scan run config version prompt doctor context prd completions migrate cleanup watch webhook productivity plugin runner tutorial undo machine cli-spec daemon; do "$0" "$c" --help >/dev/null; done' "$RALPH_BIN"

cat >>"$REPORT" <<'EOF_PHASE2'

### Phase 2 — Queue and task workflow surfaces
EOF_PHASE2
log_cmd phase2-queue-validate "$RALPH_BIN" queue validate --no-color
log_cmd phase2-queue-list "$RALPH_BIN" queue list --no-color
log_cmd phase2-queue-next "$RALPH_BIN" queue next --with-title --no-color
log_cmd phase2-config-paths "$RALPH_BIN" config paths --no-color
log_cmd phase2-config-schema "$RALPH_BIN" config schema --no-color
log_cmd phase2-config-profiles "$RALPH_BIN" config profiles list --no-color
log_cmd phase2-config-trust-init "$RALPH_BIN" config trust init --no-color
log_cmd phase2-daemon-status "$RALPH_BIN" daemon status --no-color
log_cmd phase2-daemon-logs-help "$RALPH_BIN" daemon logs --help
log_cmd phase2-task-show "$RALPH_BIN" task show RQ-0001 --no-color
log_cmd phase2-task-template-list "$RALPH_BIN" task template list --no-color
log_cmd phase2-task-decompose-preview "$RALPH_BIN" task decompose --preview --format json --runner "$RUNNER" --model "$MODEL" "Plan docs-only greeting examples" --no-color
log_cmd phase2-task-mutate-dry-run bash -c 'cat > target-mutation.json <<JSON
{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-0001","edits":[{"field":"priority","value":"high"}]}]}
JSON
"$0" task mutate --dry-run --format json --input target-mutation.json --no-color' "$RALPH_BIN"
log_cmd phase2-queue-search "$RALPH_BIN" queue search greeting --no-color
log_cmd phase2-queue-tree "$RALPH_BIN" queue tree --no-color
log_cmd phase2-queue-explain "$RALPH_BIN" queue explain --no-color
log_cmd phase2-queue-stats "$RALPH_BIN" queue stats --no-color
log_cmd phase2-queue-history "$RALPH_BIN" queue history --days 30 --no-color
log_cmd phase2-queue-burndown "$RALPH_BIN" queue burndown --days 30 --no-color
log_cmd phase2-queue-aging "$RALPH_BIN" queue aging --no-color
log_cmd phase2-queue-dashboard "$RALPH_BIN" queue dashboard --no-color
log_cmd phase2-queue-graph "$RALPH_BIN" queue graph --format dot --no-color
log_cmd phase2-queue-schema "$RALPH_BIN" queue schema --no-color
log_cmd phase2-queue-export-json "$RALPH_BIN" queue export --format json --output target-queue-export.json --no-color
log_cmd phase2-queue-import-dry-run "$RALPH_BIN" queue import --format json --input target-queue-export.json --dry-run --on-duplicate rename --no-color
log_cmd phase2-queue-repair-dry-run "$RALPH_BIN" queue repair --dry-run --no-color
log_cmd phase2-queue-prune-dry-run "$RALPH_BIN" queue prune --dry-run --keep-last 10 --no-color
log_cmd phase2-queue-unlock-inspect "$RALPH_BIN" queue unlock --dry-run --no-color
log_cmd phase2-machine-system-info "$RALPH_BIN" machine system info --no-color
log_cmd phase2-machine-config-resolve "$RALPH_BIN" machine config resolve --no-color
log_cmd phase2-machine-workspace-overview "$RALPH_BIN" machine workspace overview --no-color
log_cmd phase2-machine-queue-read "$RALPH_BIN" machine queue read --no-color
log_cmd phase2-machine-queue-validate "$RALPH_BIN" machine queue validate --no-color
log_cmd phase2-machine-queue-graph "$RALPH_BIN" machine queue graph --no-color
log_cmd phase2-machine-queue-dashboard "$RALPH_BIN" machine queue dashboard --no-color
log_cmd phase2-machine-queue-repair-dry-run "$RALPH_BIN" machine queue repair --dry-run --no-color
log_cmd phase2-machine-queue-undo "$RALPH_BIN" machine queue undo --list --no-color
log_cmd phase2-machine-queue-unlock-inspect "$RALPH_BIN" machine queue unlock-inspect --no-color
log_cmd phase2-machine-doctor-report "$RALPH_BIN" machine doctor report --no-color
log_cmd phase2-machine-schema "$RALPH_BIN" machine schema --no-color
log_cmd phase2-machine-cli-spec "$RALPH_BIN" machine cli-spec --no-color
log_cmd phase2-prompt-list "$RALPH_BIN" prompt list --no-color
log_cmd phase2-prompt-show-worker "$RALPH_BIN" prompt show worker --raw --no-color
log_cmd phase2-prompt-scan "$RALPH_BIN" prompt scan --focus "dogfood prompt scan" --repo-prompt off --no-color
log_cmd phase2-prompt-task-builder "$RALPH_BIN" prompt task-builder --request "Add greeting docs" --tags docs --scope README.md --repo-prompt off --no-color
log_cmd phase2-prompt-export-worker "$RALPH_BIN" prompt export worker --no-color
log_cmd phase2-prompt-sync-dry-run "$RALPH_BIN" prompt sync --dry-run --no-color
log_cmd phase2-prompt-diff "$RALPH_BIN" prompt diff worker --no-color
log_cmd phase2-context-validate "$RALPH_BIN" context validate --no-color
log_cmd phase2-context-update-dry-run bash -c 'echo "Dogfood learning: keep fixture CI in scripts/ci.sh." > target-learnings.md; "$0" context update --dry-run --section troubleshooting --file target-learnings.md --no-color' "$RALPH_BIN"
log_cmd phase2-prd-create-dry-run "$RALPH_BIN" prd create docs/prd/named-greeting.md --dry-run --multi --no-color
log_cmd phase2-migrate-status "$RALPH_BIN" migrate status --no-color
log_cmd phase2-migrate-check "$RALPH_BIN" migrate --check --no-color
log_cmd phase2-cleanup-dry-run "$RALPH_BIN" cleanup --dry-run --no-color
log_cmd phase2-webhook-status "$RALPH_BIN" webhook status --format json --no-color
log_cmd phase2-webhook-test-print-json "$RALPH_BIN" webhook test --event phase_started --url https://example.com/webhook --print-json --no-color
log_cmd phase2-productivity-summary "$RALPH_BIN" productivity summary --no-color
log_cmd phase2-productivity-velocity "$RALPH_BIN" productivity velocity --no-color
log_cmd phase2-productivity-streak "$RALPH_BIN" productivity streak --no-color
log_cmd phase2-productivity-estimation "$RALPH_BIN" productivity estimation --no-color
log_cmd phase2-plugin-list "$RALPH_BIN" plugin list --no-color
log_cmd phase2-plugin-validate "$RALPH_BIN" plugin validate --no-color
log_cmd phase2-runner-capabilities-pi "$RALPH_BIN" runner capabilities pi --format json --no-color
log_cmd phase2-runner-list-json "$RALPH_BIN" runner list --format json --no-color
log_cmd phase2-task-field-write "$RALPH_BIN" task field dogfood_probe true RQ-0001 --no-color
log_cmd phase2-undo-list "$RALPH_BIN" undo --list --no-color
log_cmd phase2-undo-dry-run "$RALPH_BIN" undo --dry-run --no-color
log_cmd phase2-run-loop-dry-run "$RALPH_BIN" run loop --dry-run --max-tasks 1 --phases "$PHASES" --runner "$RUNNER" --model "$MODEL" --git-publish-mode commit --non-interactive --no-color
log_cmd phase2-dry-run "$RALPH_BIN" run one --dry-run --id RQ-0001 --phases "$PHASES" --runner "$RUNNER" --model "$MODEL" --git-publish-mode commit --non-interactive --no-color
log_cmd phase2-app-open-help "$RALPH_BIN" app open --help
log_cmd phase2-commit-runtime git add -f AGENTS.md .ralph/config.jsonc .ralph/queue.jsonc .ralph/done.jsonc .ralph/README.md .ralph/prompts/worker.md .gitignore target-queue-export.json target-mutation.json target-learnings.md
log_cmd phase2-commit-runtime-state git commit -m "Initialize Ralph dogfood runtime"

cat >>"$REPORT" <<'EOF_PHASE3'

### Phase 3 — Real three-phase agent execution
EOF_PHASE3
if [[ "$RUN_REAL_AGENT" -eq 1 ]]; then
  log_cmd_allow_fail phase3-run-one "$RALPH_BIN" run one --id RQ-0001 --phases "$PHASES" --runner "$RUNNER" --model "$MODEL" --git-publish-mode commit --non-interactive --debug --no-progress --no-color
  PHASE3_STATUS=$?
  log_cmd_allow_fail phase3-post-ci ./scripts/ci.sh || true
  log_cmd_allow_fail phase3-post-queue-validate "$RALPH_BIN" queue validate --no-color || true
  log_cmd_allow_fail phase3-git-status git status --short --branch || true
  log_cmd_allow_fail phase3-git-log git log --oneline --decorate -5 || true
  if [[ "$PHASE3_STATUS" -ne 0 ]]; then
    echo "" >>"$REPORT"
    echo "Phase 3 real agent run failed; inspect logs/phase3-run-one.log and the fixture repo for a reproducible Ralph issue." >>"$REPORT"
    exit "$PHASE3_STATUS"
  fi
else
  echo "- SKIP phase3-run-one — --skip-real-agent selected" >>"$REPORT"
fi

cat >>"$REPORT" <<EOF_DONE

## Artifacts

- Fixture project: $PROJECT_DIR
- Logs: $LOG_DIR
- Report: $REPORT

## Repeat Command

\`scripts/dogfood-ralph.sh --runner '$RUNNER' --model '$REQUESTED_MODEL' --phases '$PHASES'\`
EOF_DONE

CURRENT_STEP="complete"
echo "Dogfood completed: $REPORT"
if [[ "$KEEP_PROJECT" -eq 1 ]]; then
  echo "Fixture project retained: $PROJECT_DIR"
fi
