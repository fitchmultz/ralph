#!/usr/bin/env bash
# Cursor Agent CLI smoke test for Ralph integration (stream-json + resume).
#
# Requirements:
# - `agent` on PATH (Cursor Agent CLI)
# - CURSOR_API_KEY or interactive login available to the agent
#
# Model: composer-2 only (project policy for this smoke script).
#
# Usage:
#   ./scripts/cursor-agent-runner-smoke.sh [WORKDIR]
#
set -euo pipefail

WORKDIR="${1:-$(pwd)}"
MODEL="composer-2"
BIN="${CURSOR_AGENT_BIN:-agent}"

if ! command -v "$BIN" >/dev/null 2>&1; then
  echo "error: '$BIN' not found on PATH (set CURSOR_AGENT_BIN to override)" >&2
  exit 2
fi

if [[ -z "${CURSOR_API_KEY:-}" ]]; then
  echo "warning: CURSOR_API_KEY is unset; agent may prompt for login" >&2
fi

OUT="$(mktemp -t ralph-cursor-smoke-out.XXXXXX)"
ERR="$(mktemp -t ralph-cursor-smoke-err.XXXXXX)"
cleanup() {
  rm -f "$OUT" "$ERR"
}
trap cleanup EXIT

echo "== Cursor agent version"
"$BIN" --version

echo "== stream-json run + session capture (model=$MODEL)"
cd "$WORKDIR"
set +e
"$BIN" -p --trust --output-format stream-json --model "$MODEL" \
  "Reply with exactly: CURSOR_SMOKE_SESSION" >"$OUT" 2>"$ERR"
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  echo "error: initial agent run failed (exit $status)" >&2
  cat "$ERR" >&2 || true
  exit 1
fi

SESSION_ID="$(
  python3 - "$OUT" <<'PY'
import json
import sys

path = sys.argv[1]
last = None
with open(path, "r", encoding="utf-8") as handle:
    for raw in handle:
        line = raw.strip()
        if not line:
            continue
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        session_id = payload.get("session_id")
        if isinstance(session_id, str) and session_id.strip():
            last = session_id.strip()

if not last:
    sys.exit(2)
print(last)
PY
)"

if [[ "$SESSION_ID" == "" ]]; then
  echo "error: could not extract session_id from stream-json output" >&2
  tail -n 50 "$OUT" >&2 || true
  exit 1
fi

echo "== resume session ($SESSION_ID)"
set +e
"$BIN" -p --trust --output-format stream-json --model "$MODEL" \
  --resume "$SESSION_ID" "Reply with exactly: CURSOR_SMOKE_RESUME" >"$OUT" 2>"$ERR"
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  echo "error: resume run failed (exit $status)" >&2
  cat "$ERR" >&2 || true
  exit 1
fi

if ! grep -q "CURSOR_SMOKE_RESUME" "$OUT"; then
  echo "error: expected resume output to mention CURSOR_SMOKE_RESUME" >&2
  tail -n 50 "$OUT" >&2 || true
  exit 1
fi

echo "ok: cursor agent stream-json + resume smoke passed"
