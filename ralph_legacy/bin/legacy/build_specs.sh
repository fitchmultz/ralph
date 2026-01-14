#!/usr/bin/env bash
# Purpose: Generate or refresh specs using Codex (default) or opencode with a prompt template.
# Entrypoint: build_specs.sh
# Notes: Uses Codex defaults unless overridden; opencode can be selected via --runner.

set -euo pipefail


die() {
  echo "Error: $*" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Generate or refresh Ralph specs using Codex (default) with a prompt template.

Usage:
  ralph_legacy/bin/legacy/build_specs.sh [options] [-- <runner args>]

Options:
  --runner NAME          Runner to use: codex or opencode (default: codex)
  --prompt PATH          Prompt template (default: ralph_legacy/specs/specs_builder.md)
  --interactive          Ask for user input before adding new queue items (uses interactive codex)
  --innovate             Allow the specs builder to add new items directly to `## Queue`
  --autofill-scout       Enable auto-innovate when the Queue is empty (overrides script default)
  --no-autofill-scout    Disable auto-innovate when the Queue is empty (overrides script default)
  --print-prompt         Print the filled prompt and exit
  -h, --help             Show this help message

Examples:
  ralph_legacy/bin/legacy/build_specs.sh
  ralph_legacy/bin/legacy/build_specs.sh --print-prompt
  ralph_legacy/bin/legacy/build_specs.sh --interactive
  ralph_legacy/bin/legacy/build_specs.sh --innovate
  ralph_legacy/bin/legacy/build_specs.sh --runner opencode
  ralph_legacy/bin/legacy/build_specs.sh --runner opencode -- --agent default
  ralph_legacy/bin/legacy/build_specs.sh --no-autofill-scout
USAGE
}

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [[ -z "$repo_root" ]]; then
  repo_root="$(cd "${script_dir}/../.." && pwd)"
fi
lock_base="${TMPDIR:-/tmp}"
lock_id="$(printf '%s' "$repo_root" | cksum | awk '{print $1}')"
lock_dir="${lock_base%/}/ralph.lock.${lock_id}"
lock_pid_file="${lock_dir}/owner.pid"

PROMPT_TEMPLATE="${repo_root}/ralph_legacy/specs/specs_builder.md"
RUNNER="codex"
PRINT_PROMPT=0
RUNNER_ARGS=()
INTERACTIVE=0

# Autofill/scout default policy (single toggle).
# Flip this ONE value when you want build_specs to aggressively seed an empty Queue:
#   1 = auto-enable innovation/scout when ## Queue has 0 unchecked items
#   0 = never auto-enable innovation/scout (only enable via --innovate)
RALPH_AUTOFILL_SCOUT_DEFAULT=1

AUTOFILL_SCOUT="${RALPH_AUTOFILL_SCOUT_DEFAULT}"
INNOVATE=0
INNOVATE_EXPLICIT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --runner)
      RUNNER="$2"
      shift 2
      ;;
    --prompt)
      PROMPT_TEMPLATE="$2"
      shift 2
      ;;
    --interactive)
      INTERACTIVE=1
      shift
      ;;
    --innovate)
      INNOVATE=1
      INNOVATE_EXPLICIT=1
      shift
      ;;
    --autofill-scout)
      AUTOFILL_SCOUT=1
      shift
      ;;
    --no-autofill-scout)
      AUTOFILL_SCOUT=0
      shift
      ;;
    --print-prompt)
      PRINT_PROMPT=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      RUNNER_ARGS+=("$@")
      break
      ;;
    *)
      die "Unknown argument: $1"
      ;;
  esac
done

case "$RUNNER" in
  codex)
    if ! command -v codex >/dev/null 2>&1; then
      die "codex is not on PATH. Install it or use --runner opencode."
    fi
    ;;
  opencode)
    if ! command -v opencode >/dev/null 2>&1; then
      die "opencode is not on PATH. Install it or use --runner codex."
    fi
    ;;
  *)
    die "--runner must be codex or opencode (got: $RUNNER)"
    ;;
esac

if [[ ! -f "$PROMPT_TEMPLATE" ]]; then
  die "Prompt template not found: $PROMPT_TEMPLATE"
fi

if ! grep -q "AGENTS\\.md" "$PROMPT_TEMPLATE"; then
  die "Prompt template must reference AGENTS.md (root): $PROMPT_TEMPLATE"
fi

PYTHON_CMD=()
if command -v uv >/dev/null 2>&1; then
  PYTHON_CMD=(uv run --project "${repo_root}/ralph_legacy" python)
elif command -v python3 >/dev/null 2>&1; then
  PYTHON_CMD=(python3)
else
  die "python3 (or uv) is required to build the prompt"
fi

cleanup_paths=()
TMP_PROMPT_FILE=""
lock_acquired=0

cleanup() {
  if [[ "$lock_acquired" -eq 1 ]]; then
    rm -rf "$lock_dir" 2>/dev/null || true
  fi
  for path in "${cleanup_paths[@]}"; do
    rm -f "$path"
  done
}

trap cleanup EXIT

is_pid_running() {
  local pid="$1"
  if [[ -z "$pid" ]]; then
    return 1
  fi
  ps -p "$pid" >/dev/null 2>&1
}

is_ancestor_pid() {
  local ancestor_pid="$1"
  local current_pid="$$"
  while [[ -n "$current_pid" && "$current_pid" -gt 1 ]]; do
    if [[ "$current_pid" -eq "$ancestor_pid" ]]; then
      return 0
    fi
    current_pid=$(ps -o ppid= -p "$current_pid" 2>/dev/null | tr -d ' ')
    if [[ -z "$current_pid" ]]; then
      break
    fi
  done
  return 1
}

acquire_lock() {
  if mkdir "$lock_dir" 2>/dev/null; then
    echo "$$" > "$lock_pid_file"
    lock_acquired=1
    return 0
  fi

  if [[ -f "$lock_pid_file" ]]; then
    owner_pid=$(cat "$lock_pid_file" 2>/dev/null || true)
    if [[ -n "$owner_pid" ]] && is_ancestor_pid "$owner_pid"; then
      return 0
    fi
    if [[ -n "$owner_pid" ]] && ! is_pid_running "$owner_pid"; then
      rm -rf "$lock_dir" 2>/dev/null || true
      if mkdir "$lock_dir" 2>/dev/null; then
        echo "$$" > "$lock_pid_file"
        lock_acquired=1
        return 0
      fi
    fi
  else
    die "Ralph lock exists but owner pid file is missing. Run ralph_legacy/bin/legacy/ralph_unlock.sh."
  fi

  die "Another Ralph process is running (lock: $lock_dir)."
}

acquire_lock

if [[ -z "$TMP_PROMPT_FILE" ]]; then
  TMP_PROMPT_FILE=$(mktemp)
  cleanup_paths+=("$TMP_PROMPT_FILE")
fi

unchecked_queue_count() {
  local queue_path="$1"
  awk '
    BEGIN { in_queue = 0; count = 0 }
    /^## Queue[[:space:]]*$/ { in_queue = 1; next }
    /^## / { in_queue = 0 }
    in_queue && $0 ~ /^[[:space:]]*- \[ \]/ { count += 1 }
    END { print count + 0 }
  ' "$queue_path"
}

# Auto-enable innovate when the queue is empty (opt-in via a single toggle).
queue_path="${repo_root}/ralph_legacy/specs/implementation_queue.md"
if [[ "$INNOVATE_EXPLICIT" -eq 0 && "$AUTOFILL_SCOUT" -eq 1 ]]; then
  if [[ ! -f "$queue_path" ]]; then
    INNOVATE=1
  else
    queue_items="$(unchecked_queue_count "$queue_path")"
    if [[ "$queue_items" -eq 0 ]]; then
      INNOVATE=1
    fi
  fi
fi

"${PYTHON_CMD[@]}" - <<'PY' "$PROMPT_TEMPLATE" "$TMP_PROMPT_FILE" "$INTERACTIVE" "$INNOVATE"
from pathlib import Path
import sys

template_path = Path(sys.argv[1])
output_path = Path(sys.argv[2])
interactive = sys.argv[3] == "1"
innovate = sys.argv[4] == "1"

content = template_path.read_text()
interactive_placeholder = "{{INTERACTIVE_INSTRUCTIONS}}"
interactive_instructions = ""
if interactive:
    interactive_instructions = (
        "INTERACTIVE MODE ENABLED. Before adding any new queue items:\\n"
        "1) List the candidate items you intend to add (bulleted).\\n"
        "2) Ask the user for directives/approval or edits.\\n"
        "3) Wait for the user's response, then incorporate it.\\n"
        "If no new items are proposed, ask the user if they want any new directions.\\n"
    )
if interactive_placeholder in content:
    content = content.replace(interactive_placeholder, interactive_instructions)
elif interactive:
    raise SystemExit("Error: Prompt template missing {{INTERACTIVE_INSTRUCTIONS}} placeholder")

innovate_placeholder = "{{INNOVATE_INSTRUCTIONS}}"
innovate_instructions = ""
if innovate:
    innovate_instructions = (
        "AUTOFILL/SCOUT MODE ENABLED (AGGRESSIVE).\\n"
        "\\n"
        "This repo intentionally avoids TODO/TBD placeholders. You must rely on 'AI vibes' grounded in real repo signals:\\n"
        "- duplicated logic across tools/backends\\n"
        "- inconsistent CLI contracts / help/docstring standards\\n"
        "- missing shared helpers that should live under backend/idf/\\n"
        "- workflow gaps in Makefile/composite pipelines\\n"
        "- missing regression coverage for brittle logic\\n"
        "\\n"
        "Mandatory scouting (repo_prompt):\\n"
        "- Start by calling get_file_tree.\\n"
        "- Then read a small but representative set of files across backend/, tools/, frontend/, and ops/.\\n"
        "\\n"
        "Queue seeding rule:\\n"
        "- If `## Queue` is empty, you MUST populate it with 10-15 high-leverage, outcome-sized items.\\n"
        "\\n"
        "Evidence requirement for NEW items:\\n"
        "- Each item must cite concrete file paths and what you observed (function/class/pattern), or a concrete Make target/workflow gap.\\n"
        "- Do not invent evidence; only claim what you can point to in the repo.\\n"
    )
if innovate_placeholder in content:
    content = content.replace(innovate_placeholder, innovate_instructions)
elif innovate:
    raise SystemExit("Error: Prompt template missing {{INNOVATE_INSTRUCTIONS}} placeholder")

filled = content
output_path.write_text(filled)
PY

if [[ "$PRINT_PROMPT" -eq 1 ]]; then
  cat "$TMP_PROMPT_FILE"
  exit 0
fi

run_args=("${RUNNER_ARGS[@]}")
if [[ "$RUNNER" == "codex" ]]; then
  if [[ "$INTERACTIVE" -eq 1 ]]; then
    prompt_size=$(wc -c < "$TMP_PROMPT_FILE" | tr -d ' ')
    if [[ "$prompt_size" -gt 200000 ]]; then
      die "Prompt too large for interactive codex (size: $prompt_size bytes). Use non-interactive or opencode."
    fi
    prompt_content=$(cat "$TMP_PROMPT_FILE")
    if ! codex "${run_args[@]}" "$prompt_content"; then
      die "codex failed while building specs."
    fi
  else
    if ! codex exec "${run_args[@]}" - < "$TMP_PROMPT_FILE"; then
      die "codex failed while building specs."
    fi
  fi
else
  if [[ "$INTERACTIVE" -eq 1 ]]; then
    prompt_size=$(wc -c < "$TMP_PROMPT_FILE" | tr -d ' ')
    if [[ "$prompt_size" -gt 200000 ]]; then
      die "Prompt too large for interactive opencode (size: $prompt_size bytes). Use non-interactive or codex."
    fi
    prompt_content=$(cat "$TMP_PROMPT_FILE")
    if ! opencode "${run_args[@]}" "$prompt_content"; then
      die "opencode failed while building specs."
    fi
  else
    if ! opencode run "${run_args[@]}" --file "$TMP_PROMPT_FILE" -- "Follow the attached prompt file verbatim."; then
      die "opencode failed while building specs."
    fi
  fi
fi
