#!/usr/bin/env bash
#
# Purpose: Deterministically smoke-test macOS Settings presentation paths.
# Responsibilities:
# - Launch the built RalphMac app with disposable workspaces and diagnostics capture enabled.
# - Exercise command-surface, app-menu, and URL-routed Settings entry paths.
# - Fail if Settings reintroduces helper windows, text-view first-responder fallback, or workspace-retarget drift.
# Scope:
# - Local macOS smoke verification only; it does not build the app by itself.
# Usage:
# - scripts/macos-settings-smoke.sh
# - scripts/macos-settings-smoke.sh --app-bundle target/tmp/xcode-deriveddata/build/Build/Products/Release/RalphMac.app
# Invariants/assumptions:
# - Requires macOS with `open`, `osascript`, `launchctl`, `peekaboo`, and `python3` available.
# - The app bundle contains the companion `ralph` CLI at `Contents/MacOS/ralph`.

set -euo pipefail

APP_BUNDLE="target/tmp/xcode-deriveddata/build/Build/Products/Release/RalphMac.app"
APP_NAME="RalphMac"

usage() {
    cat <<'EOF'
Usage:
  scripts/macos-settings-smoke.sh [--app-bundle <path>]

Options:
  --app-bundle <path>   RalphMac.app bundle to launch
  -h, --help            Show this help text
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --app-bundle)
            APP_BUNDLE="$2"
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
APP_BUNDLE="$(cd "$(dirname "$APP_BUNDLE")" && pwd)/$(basename "$APP_BUNDLE")"
APP_CLI="$APP_BUNDLE/Contents/MacOS/ralph"

for command in open osascript launchctl peekaboo python3; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "ERROR: required command not found: $command" >&2
        exit 2
    fi
done

if [ ! -d "$APP_BUNDLE" ]; then
    echo "ERROR: app bundle not found: $APP_BUNDLE" >&2
    exit 2
fi
if [ ! -x "$APP_CLI" ]; then
    echo "ERROR: bundled ralph CLI not found or not executable: $APP_CLI" >&2
    exit 2
fi

TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/ralph-settings-smoke.XXXXXX")"
WORKSPACE_A="$TEMP_ROOT/workspace-a"
WORKSPACE_B="$TEMP_ROOT/workspace-b"
DIAGNOSTICS_PATH="$TEMP_ROOT/settings-diagnostics.json"
LAST_SNAPSHOT_PATH="$TEMP_ROOT/last-snapshot.json"

cleanup() {
    osascript -e 'tell application "RalphMac" to quit' >/dev/null 2>&1 || true
    pkill -f 'RalphMac.app/Contents/MacOS/RalphMac' >/dev/null 2>&1 || true
    launchctl unsetenv RALPH_UI_TEST_WORKSPACE_PATH >/dev/null 2>&1 || true
    launchctl unsetenv RALPH_SETTINGS_DIAGNOSTICS_PATH >/dev/null 2>&1 || true
    launchctl unsetenv RALPH_BIN_PATH >/dev/null 2>&1 || true
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

wait_for_workspace_window() {
    python3 - "$APP_NAME" <<'PY'
import json
import subprocess
import sys
import time

app_name = sys.argv[1]
deadline = time.time() + 20
last_output = ""
while time.time() < deadline:
    proc = subprocess.run(
        ["peekaboo", "list", "windows", "--app", app_name, "--json"],
        capture_output=True,
        text=True,
        check=False,
    )
    last_output = proc.stdout or proc.stderr
    if proc.returncode == 0 and proc.stdout:
        data = json.loads(proc.stdout)
        count = data.get("data", {}).get("targetApplication", {}).get("windowCount", 0)
        if count >= 1:
            print(f"workspace-window-count={count}")
            sys.exit(0)
    time.sleep(0.25)
print(last_output, file=sys.stderr)
sys.exit(1)
PY
}

wait_for_snapshot() {
    local expected_sequence="$1"
    local expected_source="$2"
    local expected_workspace="$3"
    local expected_model="$4"

    python3 - "$DIAGNOSTICS_PATH" "$LAST_SNAPSHOT_PATH" "$expected_sequence" "$expected_source" "$expected_workspace" "$expected_model" <<'PY'
import json
import os
import pathlib
import sys
import time

path = pathlib.Path(sys.argv[1])
last_path = pathlib.Path(sys.argv[2])
expected_sequence = int(sys.argv[3])
expected_source = sys.argv[4]
expected_workspace = os.path.realpath(sys.argv[5])
expected_model = sys.argv[6]


def normalize_path(value):
    if not value:
        return value
    return os.path.realpath(value)


deadline = time.time() + 25
last_payload = None
last_failures = None

while time.time() < deadline:
    if path.exists():
        try:
            payload = json.loads(path.read_text())
        except json.JSONDecodeError:
            time.sleep(0.2)
            continue
        last_payload = payload
        if payload.get("requestSequence", 0) < expected_sequence:
            time.sleep(0.2)
            continue

        resolved_workspace = normalize_path(payload.get("resolvedWorkspacePath"))
        content_workspace = normalize_path(payload.get("contentWorkspacePath"))

        failures = []
        if payload.get("requestSequence") != expected_sequence:
            failures.append(f"requestSequence={payload.get('requestSequence')} expected {expected_sequence}")
        if payload.get("source") != expected_source:
            failures.append(f"source={payload.get('source')} expected {expected_source}")
        if resolved_workspace != expected_workspace:
            failures.append(f"resolvedWorkspacePath={payload.get('resolvedWorkspacePath')} expected {expected_workspace}")
        if content_workspace != expected_workspace:
            failures.append(f"contentWorkspacePath={payload.get('contentWorkspacePath')} expected {expected_workspace}")
        if payload.get("visibleAppWindowCount") != 2:
            failures.append(f"visibleAppWindowCount={payload.get('visibleAppWindowCount')} expected 2")
        if payload.get("visibleWorkspaceWindowCount") != 1:
            failures.append(f"visibleWorkspaceWindowCount={payload.get('visibleWorkspaceWindowCount')} expected 1")
        if payload.get("visibleSettingsWindowCount") != 1:
            failures.append(f"visibleSettingsWindowCount={payload.get('visibleSettingsWindowCount')} expected 1")
        if payload.get("visibleHelperWindowCount") != 0:
            failures.append(f"visibleHelperWindowCount={payload.get('visibleHelperWindowCount')} expected 0")
        if payload.get("firstResponderIsTextView") is not False:
            failures.append("firstResponderIsTextView should be false")
        if payload.get("settingsWindowIsKey") is not True:
            failures.append("settingsWindowIsKey should be true")
        if payload.get("settingsIsLoading") is not False:
            failures.append("settingsIsLoading should be false")
        if expected_model and payload.get("settingsModelValue") != expected_model:
            failures.append(f"settingsModelValue={payload.get('settingsModelValue')} expected {expected_model}")

        if not failures:
            rendered = json.dumps(payload, sort_keys=True)
            last_path.write_text(rendered + "\n")
            print(rendered)
            sys.exit(0)

        last_failures = failures
    time.sleep(0.2)

if last_payload is not None:
    if last_failures:
        print("Snapshot never satisfied invariants:", file=sys.stderr)
        for failure in last_failures:
            print(f"- {failure}", file=sys.stderr)
    print(json.dumps(last_payload, indent=2, sort_keys=True), file=sys.stderr)
else:
    print(f"No diagnostics written to {path}", file=sys.stderr)
sys.exit(1)
PY
}

open_settings_from_app_menu() {
    osascript <<'APPLESCRIPT'
    tell application "System Events"
        tell process "RalphMac"
            set frontmost to true
            click menu bar item "RalphMac" of menu bar 1
            set appMenu to menu "RalphMac" of menu bar item "RalphMac" of menu bar 1
            repeat with candidate in {"Settings…", "Settings...", "Preferences…", "Preferences..."}
                if exists menu item candidate of appMenu then
                    click menu item candidate of appMenu
                    return
                end if
            end repeat
            error "Settings menu item not found"
        end tell
    end tell
APPLESCRIPT
}

deliver_url_to_running_app() {
    local url="$1"
    osascript <<APPLESCRIPT
    tell application "RalphMac"
        activate
        open location "$url"
    end tell
APPLESCRIPT
}

open_workspace_url() {
    local workspace_path="$1"
    local encoded_path
    encoded_path="$(python3 - "$workspace_path" <<'PY'
import sys, urllib.parse
print(urllib.parse.quote(sys.argv[1]))
PY
)"
    deliver_url_to_running_app "ralph://open?workspace=$encoded_path"
}

prepare_workspace "$WORKSPACE_A"
prepare_workspace "$WORKSPACE_B"

WORKSPACE_A="$(canonicalize_path "$WORKSPACE_A")"
WORKSPACE_B="$(canonicalize_path "$WORKSPACE_B")"

write_workspace_b_config

rm -f "$DIAGNOSTICS_PATH" "$LAST_SNAPSHOT_PATH"
osascript -e 'tell application "RalphMac" to quit' >/dev/null 2>&1 || true
pkill -f 'RalphMac.app/Contents/MacOS/RalphMac' >/dev/null 2>&1 || true
launchctl setenv RALPH_UI_TEST_WORKSPACE_PATH "$WORKSPACE_A"
launchctl setenv RALPH_SETTINGS_DIAGNOSTICS_PATH "$DIAGNOSTICS_PATH"
launchctl setenv RALPH_BIN_PATH "$APP_CLI"
open -na "$APP_BUNDLE" --args --uitesting

wait_for_workspace_window >/dev/null
sleep 2

osascript -e 'tell application "System Events" to keystroke "," using command down'
SNAPSHOT_1="$(wait_for_snapshot 1 command-surface "$WORKSPACE_A" '')"
echo "✓ command-surface keyboard open: $SNAPSHOT_1"

open_settings_from_app_menu
SNAPSHOT_2="$(wait_for_snapshot 2 command-surface "$WORKSPACE_A" '')"
echo "✓ command-surface app-menu open: $SNAPSHOT_2"

open_workspace_url "$WORKSPACE_B"
sleep 2
deliver_url_to_running_app 'ralph://settings'
SNAPSHOT_3="$(wait_for_snapshot 3 url-scheme "$WORKSPACE_B" 'gemini-1.5-pro')"
echo "✓ url-route retarget open: $SNAPSHOT_3"

echo "Settings smoke test passed. Last snapshot:"
cat "$LAST_SNAPSHOT_PATH"
