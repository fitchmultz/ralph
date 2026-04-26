#!/usr/bin/env bash
#
# Purpose: Deterministically smoke-test macOS Settings presentation paths without hijacking the desktop.
# Responsibilities:
# - Launch the built RalphMac app in noninteractive contract mode with disposable workspaces.
# - Exercise command-surface, app-menu, and URL-routed Settings entry paths in-process.
# - Fail if Settings reintroduces helper windows, placeholder-retarget drift, or config-loading regressions.
# Scope:
# - Local macOS smoke verification only; it does not build the app by itself.
# Usage:
# - scripts/macos-settings-smoke.sh
# - scripts/macos-settings-smoke.sh --app-bundle target/tmp/xcode-deriveddata/build/Build/Products/Release/RalphMac.app
# Invariants/assumptions:
# - Requires macOS with `python3` available.
# - The app bundle contains the companion `ralph` CLI at `Contents/MacOS/ralph`.
# - Contract mode launches the app executable directly with `--settings-smoke-contract`; it must never rely on `open`, AppleScript, or interactive focus stealing.

set -euo pipefail

APP_BUNDLE="target/tmp/xcode-deriveddata/build/Build/Products/Release/RalphMac.app"
APP_NAME="RalphMac"
TIMEOUT_SECONDS="90"

usage() {
    cat <<'EOF'
Usage:
  scripts/macos-settings-smoke.sh [--app-bundle <path>] [--timeout <seconds>]

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

TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ralph-settings-smoke.XXXXXX")"
WORKSPACE_A="$TEMP_ROOT/workspace-a"
WORKSPACE_B="$TEMP_ROOT/workspace-b"
REPORT_PATH="$TEMP_ROOT/settings-contract-report.json"
APP_LOG="$TEMP_ROOT/settings-contract.log"
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

write_workspace_b_config() {
    mkdir -p "$WORKSPACE_B/.ralph"
    cat > "$WORKSPACE_B/.ralph/config.jsonc" <<'EOF'
{
  "agent": {
    "runner": "gemini",
    "model": "gemini-1.5-pro",
    "phases": 3,
    "iterations": 1,
    "reasoning_effort": "medium"
  }
}
EOF
}

prepare_workspace "$WORKSPACE_A"
prepare_workspace "$WORKSPACE_B"

WORKSPACE_A="$(canonicalize_path "$WORKSPACE_A")"
WORKSPACE_B="$(canonicalize_path "$WORKSPACE_B")"

write_workspace_b_config

rm -f "$REPORT_PATH" "$APP_LOG"

RALPH_SETTINGS_SMOKE_WORKSPACE_A="$WORKSPACE_A" \
RALPH_SETTINGS_SMOKE_WORKSPACE_B="$WORKSPACE_B" \
RALPH_SETTINGS_SMOKE_REPORT_PATH="$REPORT_PATH" \
"$APP_EXECUTABLE" --settings-smoke-contract >"$APP_LOG" 2>&1 &
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
    echo "ERROR: Settings smoke contract timed out after ${TIMEOUT_SECONDS}s" >&2
    echo "--- app log ---" >&2
    cat "$APP_LOG" >&2
    exit 1
fi

wait "$APP_PID" || app_exit="$?"
app_exit="${app_exit:-0}"
APP_PID=""

if pgrep -f "$APP_EXECUTABLE" >/dev/null 2>&1; then
    echo "ERROR: settings smoke left a lingering app process: $APP_EXECUTABLE" >&2
    ps -axo pid=,command= | grep "$APP_EXECUTABLE" | grep -v grep >&2 || true
    exit 1
fi

if [ ! -f "$REPORT_PATH" ]; then
    echo "ERROR: Settings smoke contract did not write report: $REPORT_PATH" >&2
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

expected_steps = ["keyboard-shortcut", "app-menu", "url-scheme"]
actual_steps = [step.get("name") for step in report.get("steps", [])]

if app_exit != 0:
    print(f"ERROR: contract app exited with status {app_exit}", file=sys.stderr)
    print(log_path.read_text(), file=sys.stderr)
    sys.exit(1)

if not report.get("passed"):
    print("ERROR: settings smoke contract reported failure", file=sys.stderr)
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

print("Settings smoke contract passed. Final report:")
print(report_path.read_text())
PY
