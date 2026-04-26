#!/usr/bin/env bash
#
# Purpose: Deterministically verify macOS workspace bootstrap/routing flows without hijacking the desktop.
# Responsibilities:
# - Launch the built RalphMac app in noninteractive workspace-routing contract mode with disposable workspaces.
# - Exercise bootstrap URL-open retargeting, existing-workspace URL focus, and pending scene-route delivery in-process.
# - Fail if workspace routing reintroduces duplicate workspaces, extra windows, or stale scene-route delivery.
# Scope:
# - Local macOS contract verification only; it does not build the app by itself.
# Usage:
# - scripts/macos-workspace-routing-contract.sh
# - scripts/macos-workspace-routing-contract.sh --app-bundle target/tmp/xcode-deriveddata/build/Build/Products/Release/RalphMac.app
# Invariants/assumptions:
# - Requires macOS with `python3` available.
# - The app bundle contains the companion `ralph` CLI at `Contents/MacOS/ralph`.
# - Contract mode launches the app executable directly with `--workspace-routing-contract`; it must never rely on `open`, AppleScript, or interactive focus stealing.

set -euo pipefail

APP_BUNDLE="target/tmp/xcode-deriveddata/build/Build/Products/Release/RalphMac.app"
APP_NAME="RalphMac"
TIMEOUT_SECONDS="90"

usage() {
    cat <<'EOF'
Usage:
  scripts/macos-workspace-routing-contract.sh [--app-bundle <path>] [--timeout <seconds>]

Options:
  --app-bundle <path>   RalphMac.app bundle to launch
  --timeout <seconds>   Timeout for the in-app contract run (default: 90)
  -h, --help            Show this help text
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --app-bundle)
            APP_BUNDLE="$2"
            shift 2
            ;;
        --timeout)
            TIMEOUT_SECONDS="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage
            exit 2
            ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
if ! command -v python3 >/dev/null 2>&1; then
    echo "ERROR: required command not found: python3" >&2
    exit 2
fi
APP_BUNDLE="$(
    python3 - "$REPO_ROOT" "$APP_BUNDLE" <<'PY'
import os
import sys

repo_root = sys.argv[1]
path = sys.argv[2]
if not os.path.isabs(path):
    path = os.path.join(repo_root, path)
print(os.path.abspath(path))
PY
)"
APP_EXECUTABLE="$APP_BUNDLE/Contents/MacOS/$APP_NAME"
APP_CLI="$APP_BUNDLE/Contents/MacOS/ralph"

if [ ! -d "$APP_BUNDLE" ]; then
    echo "ERROR: app bundle not found: $APP_BUNDLE" >&2
    exit 2
fi
if [ ! -x "$APP_EXECUTABLE" ]; then
    echo "ERROR: app executable not found or not executable: $APP_EXECUTABLE" >&2
    exit 2
fi
if [ ! -x "$APP_CLI" ]; then
    echo "ERROR: bundled ralph CLI not found or not executable: $APP_CLI" >&2
    exit 2
fi

TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ralph-workspace-routing-contract.XXXXXX")"
WORKSPACE_A="$TEMP_ROOT/workspace-a"
WORKSPACE_B="$TEMP_ROOT/workspace-b"
WORKSPACE_C="$TEMP_ROOT/workspace-c"
REPORT_PATH="$TEMP_ROOT/workspace-routing-contract-report.json"
APP_LOG="$TEMP_ROOT/workspace-routing-contract.log"
APP_PID=""

terminate_contract_app() {
    if [ -n "$APP_PID" ] && kill -0 "$APP_PID" >/dev/null 2>&1; then
        kill "$APP_PID" >/dev/null 2>&1 || true
        wait "$APP_PID" >/dev/null 2>&1 || true
    fi
    if pgrep -f "$APP_EXECUTABLE" >/dev/null 2>&1; then
        pkill -TERM -f "$APP_EXECUTABLE" >/dev/null 2>&1 || true
        sleep 1
        pgrep -f "$APP_EXECUTABLE" >/dev/null 2>&1 && pkill -KILL -f "$APP_EXECUTABLE" >/dev/null 2>&1 || true
    fi
}

cleanup() {
    terminate_contract_app
    rm -rf "$TEMP_ROOT"
}
trap cleanup EXIT INT TERM

prepare_workspace() {
    local workspace_path="$1"
    mkdir -p "$workspace_path"
    (cd "$workspace_path" && "$APP_CLI" --no-color init --non-interactive >/dev/null)
}

canonicalize_path() {
    python3 - "$1" <<'PY'
import os
import sys

print(os.path.realpath(sys.argv[1]))
PY
}

seed_workspace_task() {
    local workspace_path="$1"
    local task_id="$2"
    local title="$3"
    local priority="$4"
    local payload_path="$TEMP_ROOT/${task_id}.json"

    python3 - "$payload_path" "$task_id" "$title" "$priority" <<'PY'
import json
import pathlib
import sys

payload_path = pathlib.Path(sys.argv[1])
task_id = sys.argv[2]
title = sys.argv[3]
priority = sys.argv[4]
payload = [{
    "id": task_id,
    "status": "todo",
    "title": title,
    "priority": priority,
    "created_at": "2026-03-07T01:00:00Z",
    "updated_at": "2026-03-07T01:00:00Z"
}]
payload_path.write_text(json.dumps(payload))
PY

    (
        cd "$workspace_path" &&
        "$APP_CLI" --no-color queue import --format json --input "$payload_path" >/dev/null
    )
}

prepare_workspace "$WORKSPACE_A"
prepare_workspace "$WORKSPACE_B"
prepare_workspace "$WORKSPACE_C"

seed_workspace_task "$WORKSPACE_A" "RQ-0100" "Workspace A Contract Task" "medium"
seed_workspace_task "$WORKSPACE_B" "RQ-0200" "Workspace B URL Route Task" "high"
seed_workspace_task "$WORKSPACE_C" "RQ-0300" "Workspace C Pending Route Task" "medium"

WORKSPACE_A="$(canonicalize_path "$WORKSPACE_A")"
WORKSPACE_B="$(canonicalize_path "$WORKSPACE_B")"
WORKSPACE_C="$(canonicalize_path "$WORKSPACE_C")"

rm -f "$REPORT_PATH" "$APP_LOG"

RALPH_WORKSPACE_ROUTING_CONTRACT_WORKSPACE_A="$WORKSPACE_A" \
RALPH_WORKSPACE_ROUTING_CONTRACT_WORKSPACE_B="$WORKSPACE_B" \
RALPH_WORKSPACE_ROUTING_CONTRACT_WORKSPACE_C="$WORKSPACE_C" \
RALPH_WORKSPACE_ROUTING_CONTRACT_REPORT_PATH="$REPORT_PATH" \
"$APP_EXECUTABLE" --workspace-routing-contract >"$APP_LOG" 2>&1 &
APP_PID="$!"

python3 - "$APP_PID" "$TIMEOUT_SECONDS" <<'PY'
import os
import signal
import sys
import time

pid = int(sys.argv[1])
timeout = float(sys.argv[2])
deadline = time.time() + timeout

while time.time() < deadline:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        sys.exit(0)
    time.sleep(0.2)

try:
    os.kill(pid, signal.SIGTERM)
except ProcessLookupError:
    sys.exit(0)

time.sleep(1)
try:
    os.kill(pid, 0)
except ProcessLookupError:
    sys.exit(124)

os.kill(pid, signal.SIGKILL)
sys.exit(124)
PY
wait_status="$?"

if [ "$wait_status" = "124" ]; then
    echo "ERROR: Workspace routing contract timed out after ${TIMEOUT_SECONDS}s" >&2
    echo "--- app log ---" >&2
    cat "$APP_LOG" >&2
    exit 1
fi

wait "$APP_PID" || app_exit="$?"
app_exit="${app_exit:-0}"
APP_PID=""

if pgrep -f "$APP_EXECUTABLE" >/dev/null 2>&1; then
    echo "ERROR: workspace-routing contract left a lingering app process: $APP_EXECUTABLE" >&2
    ps -axo pid=,command= | grep "$APP_EXECUTABLE" | grep -v grep >&2 || true
    exit 1
fi

if [ ! -f "$REPORT_PATH" ]; then
    echo "ERROR: Workspace routing contract did not write report: $REPORT_PATH" >&2
    echo "--- app log ---" >&2
    cat "$APP_LOG" >&2
    exit 1
fi

python3 - "$REPORT_PATH" "$APP_LOG" "$app_exit" <<'PY'
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
log_path = pathlib.Path(sys.argv[2])
app_exit = int(sys.argv[3])
report = json.loads(report_path.read_text())

expected_steps = [
    "initial-bootstrap",
    "url-open-bootstrap-retarget",
    "route-pending-task-detail-to-new-workspace",
    "url-open-existing-workspace-focus",
]
actual_steps = [step.get("name") for step in report.get("steps", [])]

if app_exit != 0:
    print(f"ERROR: contract app exited with status {app_exit}", file=sys.stderr)
    print(log_path.read_text(), file=sys.stderr)
    sys.exit(1)

if not report.get("passed"):
    print("ERROR: workspace routing contract reported failure", file=sys.stderr)
    print(report_path.read_text(), file=sys.stderr)
    print("--- app log ---", file=sys.stderr)
    print(log_path.read_text(), file=sys.stderr)
    sys.exit(1)

if actual_steps != expected_steps:
    print(f"ERROR: unexpected step ordering: {actual_steps} (expected {expected_steps})", file=sys.stderr)
    print(report_path.read_text(), file=sys.stderr)
    sys.exit(1)

for step in report["steps"]:
    rendered = json.dumps(step["snapshot"], sort_keys=True)
    print(f"✓ {step['name']}: {rendered}")

print("Workspace routing contract passed. Final report:")
print(report_path.read_text())
PY
