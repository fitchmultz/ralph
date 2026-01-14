#!/usr/bin/env bash
# Purpose: Remove the Ralph lock directory when a previous run crashed.
# Entrypoint: ralph_unlock.sh

set -euo pipefail

die() {
  echo "Error: $*" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Remove the Ralph lock directory if a previous run crashed.

Usage:
  ralph_legacy/bin/ralph_unlock.sh [options]

Options:
  --force              Remove the lock even if the owner PID is running
  -h, --help           Show this help message

Examples:
  ralph_legacy/bin/ralph_unlock.sh
  ralph_legacy/bin/ralph_unlock.sh --force
USAGE
}

FORCE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "Unknown argument: $1"
      ;;
  esac
done

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [[ -z "$repo_root" ]]; then
  repo_root="$(cd "${script_dir}/../.." && pwd)"
fi
lock_base="${TMPDIR:-/tmp}"
lock_id="$(printf '%s' "$repo_root" | cksum | awk '{print $1}')"
lock_dir="${lock_base%/}/ralph.lock.${lock_id}"
lock_pid_file="${lock_dir}/owner.pid"

if [[ ! -d "$lock_dir" ]]; then
  echo ">> [RALPH] No lock present (${lock_dir})."
  exit 0
fi

owner_pid=""
if [[ -f "$lock_pid_file" ]]; then
  owner_pid=$(cat "$lock_pid_file" 2>/dev/null || true)
fi

if [[ -n "$owner_pid" ]] && ps -p "$owner_pid" >/dev/null 2>&1; then
  if [[ "$FORCE" -ne 1 ]]; then
    die "Lock owned by running PID ${owner_pid}. Use --force to remove anyway."
  fi
  echo ">> [RALPH] Removing active lock owned by PID ${owner_pid} (forced)."
fi

rm -rf "$lock_dir" 2>/dev/null || true
echo ">> [RALPH] Lock removed."
