#!/usr/bin/env bash
# Purpose: Run Codex (default) or opencode in a supervised loop that enforces queue/pin invariants.
# Entrypoint: ralph_loop.sh
# Notes: Controller owns verification, commits, and quarantine; workers only edit files.

set -euo pipefail


die() {
  echo "Error: $*" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Run the Ralph loop for Codex (default) or opencode (fresh agent each iteration).

Usage:
  ralph_legacy/bin/legacy/ralph_loop.sh [options] [-- <runner args>]

Options:
  --runner NAME              Runner to use: codex or opencode (default: codex)
  --prompt PATH              Path to worker prompt file (default: repo_root/ralph_legacy/prompt.md for codex; repo_root/ralph_legacy/prompt_opencode.md for opencode)
  --supervisor-prompt PATH   Path to supervisor prompt (default: repo_root/ralph_legacy/supervisor_prompt.md)
  --sleep SECONDS            Sleep between iterations (default: 5)
  --max-iterations N         Stop after N iterations (0 = infinite, default: 0)
  --max-stalled N            Auto-block after N stalled iterations (default: 3)
  --max-repair-attempts N    Supervisor repair attempts before auto-block (default: 2)
  --only-tag TAGS            Only execute queue items tagged with [tag] (comma-separated)
  --once                     Run exactly one iteration and exit
  -h, --help                 Show this help message

Examples:
  ralph_legacy/bin/legacy/ralph_loop.sh --once
  ralph_legacy/bin/legacy/ralph_loop.sh --max-iterations 10 --sleep 2
  ralph_legacy/bin/legacy/ralph_loop.sh --max-stalled 2
  ralph_legacy/bin/legacy/ralph_loop.sh --max-repair-attempts 1
  ralph_legacy/bin/legacy/ralph_loop.sh --only-tag db
  ralph_legacy/bin/legacy/ralph_loop.sh --only-tag db,ui
  ralph_legacy/bin/legacy/ralph_loop.sh --runner opencode
  ralph_legacy/bin/legacy/ralph_loop.sh --runner opencode -- --agent default
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

PROMPT_PATH="${repo_root}/ralph_legacy/prompt.md"
SUPERVISOR_PROMPT_PATH="${repo_root}/ralph_legacy/supervisor_prompt.md"
PROMPT_PATH_SET=0
RUNNER="codex"
SLEEP_SECS=5
MAX_ITERATIONS=0
MAX_STALLED=3
MAX_REPAIR_ATTEMPTS=2
RUN_ONCE=0
RUNNER_ARGS=()
lock_acquired=0
ONLY_TAGS=""
PUSH_FAILED=0
LAST_VALIDATE_PIN_OUTPUT=""
LAST_CI_OUTPUT=""
MAIN_BRANCH=""

# Per-iteration policy state (computed for Codex only).
EFFECTIVE_CODEX_EFFORT=""
CONTEXT_BUILDER_MANDATORY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --runner)
      RUNNER="$2"
      shift 2
      ;;
    --prompt)
      PROMPT_PATH="$2"
      PROMPT_PATH_SET=1
      shift 2
      ;;
    --supervisor-prompt)
      SUPERVISOR_PROMPT_PATH="$2"
      shift 2
      ;;
    --sleep)
      SLEEP_SECS="$2"
      shift 2
      ;;
    --max-iterations)
      MAX_ITERATIONS="$2"
      shift 2
      ;;
    --max-stalled)
      MAX_STALLED="$2"
      shift 2
      ;;
    --max-repair-attempts)
      MAX_REPAIR_ATTEMPTS="$2"
      shift 2
      ;;
    --only-tag)
      ONLY_TAGS="$2"
      shift 2
      ;;
    --once)
      RUN_ONCE=1
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

if [[ "$PROMPT_PATH_SET" -eq 0 && "$RUNNER" == "opencode" ]]; then
  PROMPT_PATH="${repo_root}/ralph_legacy/prompt_opencode.md"
fi

cleanup() {
  if [[ "$lock_acquired" -eq 1 ]]; then
    rm -rf "$lock_dir" 2>/dev/null || true
  fi
}

trap cleanup EXIT

is_pid_running() {
  local pid="$1"
  if [[ -z "$pid" ]]; then
    return 1
  fi
  ps -p "$pid" >/dev/null 2>&1
}

acquire_lock() {
  if mkdir "$lock_dir" 2>/dev/null; then
    echo "$$" > "$lock_pid_file"
    lock_acquired=1
    return 0
  fi

  if [[ -f "$lock_pid_file" ]]; then
    owner_pid=$(cat "$lock_pid_file" 2>/dev/null || true)
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

if [[ ! -f "$PROMPT_PATH" ]]; then
  die "Prompt file not found: $PROMPT_PATH"
fi

if [[ ! -f "$SUPERVISOR_PROMPT_PATH" ]]; then
  die "Supervisor prompt file not found: $SUPERVISOR_PROMPT_PATH"
fi

if [[ ! "$SLEEP_SECS" =~ ^[0-9]+$ ]]; then
  die "--sleep must be an integer (got: $SLEEP_SECS)"
fi

if [[ ! "$MAX_ITERATIONS" =~ ^[0-9]+$ ]]; then
  die "--max-iterations must be an integer (got: $MAX_ITERATIONS)"
fi
if [[ ! "$MAX_STALLED" =~ ^[0-9]+$ ]]; then
  die "--max-stalled must be an integer (got: $MAX_STALLED)"
fi
if [[ ! "$MAX_REPAIR_ATTEMPTS" =~ ^[0-9]+$ ]]; then
  die "--max-repair-attempts must be an integer (got: $MAX_REPAIR_ATTEMPTS)"
fi

plan_path="${repo_root}/ralph_legacy/specs/implementation_queue.md"
if [[ ! -f "$plan_path" ]]; then
  die "Implementation queue not found: $plan_path"
fi
done_path="${repo_root}/ralph_legacy/specs/implementation_done.md"
if [[ ! -f "$done_path" ]]; then
  die "Implementation done log not found: $done_path"
fi

if [[ ! -f "${repo_root}/ralph_legacy/bin/legacy/pin_ops.py" ]]; then
  die "pin_ops.py not found at ${repo_root}/ralph_legacy/bin/legacy/pin_ops.py"
fi

pin_ops_cmd=()
if command -v uv >/dev/null 2>&1; then
  pin_ops_cmd=(uv run --project "${repo_root}/ralph_legacy" python "${repo_root}/ralph_legacy/bin/legacy/pin_ops.py")
elif command -v python3 >/dev/null 2>&1; then
  pin_ops_cmd=(python3 "${repo_root}/ralph_legacy/bin/legacy/pin_ops.py")
else
  die "python is not on PATH; required for ralph_legacy/bin/legacy/pin_ops.py"
fi

MAIN_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || true)
if [[ -z "$MAIN_BRANCH" ]]; then
  die "Unable to detect current git branch."
fi
if [[ "$MAIN_BRANCH" != "main" ]]; then
  die "Ralph loop must run on main (current: ${MAIN_BRANCH})."
fi

run_may_fail() {
  set +e
  "$@"
  local rc=$?
  set -e
  return $rc
}

first_unchecked_queue_item() {
  awk -v tags="$ONLY_TAGS" '
    /^## Queue/ {in_queue=1; next}
    /^## / {in_queue=0}
    function has_tag(line, tags,   n, i, tag) {
      if (tags == "") return 1
      n = split(tags, arr, ",")
      for (i = 1; i <= n; i++) {
        tag = arr[i]
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", tag)
        gsub(/\[/, "", tag)
        gsub(/\]/, "", tag)
        if (tag != "" && index(line, "[" tag "]") > 0) return 1
      }
      return 0
    }
    in_queue && $0 ~ /^[[:space:]]*- \[ \]/ && has_tag($0, tags) {print; exit}
  ' "$plan_path"
}

current_item_block() {
  awk -v id="$first_item_id" '
    BEGIN {found=0; start=0}
    $0 ~ /^- \[ \]/ && $0 ~ id {found=1; start=NR}
    found {
      if (NR != start && ($0 ~ /^- \[/ || $0 ~ /^## /)) {exit}
      print
    }
  ' "$plan_path"
}

move_checked_queue_items() {
  local output
  if ! output=$("${pin_ops_cmd[@]}" move-checked --queue "$plan_path" --done "$done_path" --prepend); then
    return 1
  fi
  printf '%s' "$output"
}

extract_item_id() {
  awk '
    match($0, /[A-Z0-9]{2,10}-[0-9]{4}/) {print substr($0, RSTART, RLENGTH); exit}
  ' <<<"$1"
}

extract_item_title() {
  sed -E 's/^- \[[ xX]\] [A-Z0-9]{2,10}-[0-9]{4}([[:space:]]+\[[^]]+\])*:?[[:space:]]*//' <<<"$1" | sed -E 's/[[:space:]]+$//'
}

short_reason_for_commit() {
  local reason="$1"
  reason=$(printf '%s' "$reason" | tr '\n' ' ' | sed -E 's/[[:space:]]+/ /g' | sed -E 's/[[:space:]]+$//')
  if [[ ${#reason} -gt 60 ]]; then
    reason="${reason:0:57}..."
  fi
  printf '%s' "$reason"
}

push_if_ahead() {
  if git rev-parse --abbrev-ref --symbolic-full-name @{u} >/dev/null 2>&1; then
    local ahead_count
    ahead_count=$(git rev-list --count @{u}..HEAD 2>/dev/null || echo 0)
    if [[ "${ahead_count:-0}" -gt 0 ]]; then
      echo ">> [RALPH] Pushing ${ahead_count} commit(s) to upstream..."
      if ! run_may_fail git push; then
        echo ">> [RALPH] Warning: git push failed; continuing with local commits." >&2
        PUSH_FAILED=1
        return 0
      fi
      local ahead_count_after
      ahead_count_after=$(git rev-list --count @{u}..HEAD 2>/dev/null || echo 0)
      if [[ "${ahead_count_after:-0}" -gt 0 ]]; then
        echo ">> [RALPH] Warning: push did not bring HEAD in sync (ahead by ${ahead_count_after})." >&2
        PUSH_FAILED=1
      fi
    fi
  fi
}

run_validate_pin() {
  local out_file
  out_file=$(mktemp)
  LAST_VALIDATE_PIN_OUTPUT="$out_file"
  if ! run_may_fail "${repo_root}/ralph_legacy/bin/legacy/validate_pin.sh" >"$out_file" 2>&1; then
    return 1
  fi
  return 0
}

run_make_ci() {
  local out_file
  out_file=$(mktemp)
  LAST_CI_OUTPUT="$out_file"
  if ! run_may_fail make ci >"$out_file" 2>&1; then
    return 1
  fi
  return 0
}

cleanup_iteration_artifacts() {
  local path
  for path in "$LAST_VALIDATE_PIN_OUTPUT" "$LAST_CI_OUTPUT"; do
    if [[ -n "$path" && -f "$path" ]]; then
      rm -f "$path" 2>/dev/null || true
    fi
  done
  LAST_VALIDATE_PIN_OUTPUT=""
  LAST_CI_OUTPUT=""
}

create_wip_branch_name() {
  local item_id="$1"
  local ts
  ts=$(date +%Y%m%d_%H%M%S)
  printf 'ralph/wip/%s/%s' "$item_id" "$ts"
}

quarantine_current_state() {
  local item_id="$1"
  local head_before="$2"
  local reason="$3"
  local wip_branch
  wip_branch=$(create_wip_branch_name "$item_id")

  local attempt=0
  local candidate="$wip_branch"
  while ! git checkout -b "$candidate" >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    candidate="${wip_branch}-${attempt}"
    if [[ $attempt -ge 5 ]]; then
      die "Unable to create WIP branch for ${item_id}."
    fi
  done
  wip_branch="$candidate"

  if [[ -n "$(git status --porcelain)" ]]; then
    git add -A >/dev/null 2>&1
    local short_reason
    short_reason=$(short_reason_for_commit "$reason")
    git commit -m "WIP ${item_id}: quarantine (${short_reason})" 1>&2
  fi

  git checkout "$MAIN_BRANCH" 1>&2
  git reset --hard "$head_before" 1>&2
  git clean -fd 1>&2

  printf '%s' "$wip_branch"
}

auto_block_item() {
  local item_id="$1"
  local reason="$2"
  local wip_branch="$3"
  local head_before="$4"
  local reason_short
  reason_short=$(short_reason_for_commit "$reason")

  local unblock_hint="Inspect ${wip_branch} and requeue once fixed."
  if ! run_may_fail "${pin_ops_cmd[@]}" block-item \
    --queue "$plan_path" \
    --item-id "$item_id" \
    --reason "$reason" \
    --reason "Unblock: ${unblock_hint}" \
    --wip-branch "$wip_branch" \
    --known-good "$head_before" \
    --unblock-hint "$unblock_hint"; then
    die "Failed to move ${item_id} to Blocked via pin_ops.py."
  fi

  git add "$plan_path"
  git commit -m "${item_id}: auto-block (${reason_short})"
  push_if_ahead
}

extract_effort_from_codex_config() {
  local cfg="$1"
  local value=""
  if [[ "$cfg" =~ model_reasoning_effort=\"?([A-Za-z]+)\"? ]]; then
    value="${BASH_REMATCH[1]}"
  fi
  if [[ -z "$value" ]]; then
    return 1
  fi
  printf '%s' "${value,,}"
  return 0
}

detect_effective_codex_effort() {
  local default_effort="$1"
  local detected="$default_effort"
  local idx=0
  while [[ $idx -lt ${#CURRENT_RUN_ARGS[@]} ]]; do
    local token="${CURRENT_RUN_ARGS[$idx]}"
    if [[ "$token" == "-c" ]]; then
      local cfg=""
      if [[ $((idx + 1)) -lt ${#CURRENT_RUN_ARGS[@]} ]]; then
        cfg="${CURRENT_RUN_ARGS[$((idx + 1))]}"
      fi
      local maybe=""
      if maybe="$(extract_effort_from_codex_config "$cfg")"; then
        detected="$maybe"
      fi
      idx=$((idx + 2))
      continue
    fi
    if [[ "$token" == *model_reasoning_effort* ]]; then
      local maybe2=""
      if maybe2="$(extract_effort_from_codex_config "$token")"; then
        detected="$maybe2"
      fi
    fi
    idx=$((idx + 1))
  done
  printf '%s' "$detected"
}

write_codex_context_builder_policy_block() {
  local effort="$1"
  local mandatory="$2"

  echo "# CODEX CONTEXT BUILDER POLICY"
  echo "Codex model_reasoning_effort: ${effort}"
  if [[ "$mandatory" -eq 1 ]]; then
    echo "MANDATORY: Because reasoning effort is low/off, you MUST use the repo_prompt context_builder to gather context and generate a plan BEFORE making code changes."
    echo "Execute the plan it generates."
  else
    echo "OPTIONAL: You MAY use the repo_prompt context_builder to gather context and generate a plan. It is recommended for complex items or difficult root-cause triage."
  fi
}

build_supervisor_context() {
  local stage="$1"
  local message="$2"
  local context_file
  context_file=$(mktemp)

  {
    cat "$SUPERVISOR_PROMPT_PATH"
    echo
    if [[ "$RUNNER" == "codex" && -n "${EFFECTIVE_CODEX_EFFORT}" ]]; then
      write_codex_context_builder_policy_block "$EFFECTIVE_CODEX_EFFORT" "$CONTEXT_BUILDER_MANDATORY"
      echo
    fi
    echo "# FAILURE CONTEXT"
    echo "Stage: ${stage}"
    echo "Message: ${message}"
    echo
    echo "# CURRENT QUEUE ITEM"
    printf '%s\n' "$CURRENT_ITEM_BLOCK"
    echo
    echo "# GIT STATUS"
    git status -sb
    echo
    echo "# GIT DIFF --STAT"
    git diff --stat
    echo
    echo "# GIT DIFF (truncated)"
    git diff | tail -n 400
    if [[ -n "$LAST_VALIDATE_PIN_OUTPUT" && -f "$LAST_VALIDATE_PIN_OUTPUT" ]]; then
      echo
      echo "# VALIDATE PIN OUTPUT (tail)"
      tail -n 200 "$LAST_VALIDATE_PIN_OUTPUT"
    fi
    if [[ -n "$LAST_CI_OUTPUT" && -f "$LAST_CI_OUTPUT" ]]; then
      echo
      echo "# MAKE CI OUTPUT (tail)"
      tail -n 200 "$LAST_CI_OUTPUT"
    fi
  } > "$context_file"

  printf '%s' "$context_file"
}

run_runner_with_prompt() {
  local prompt_file="$1"
  if [[ "$RUNNER" == "codex" ]]; then
    if ! codex exec "${CURRENT_RUN_ARGS[@]}" - < "$prompt_file"; then
      return 1
    fi
  else
    if ! opencode run "${CURRENT_RUN_ARGS[@]}" --file "$prompt_file" -- "Follow the attached prompt file verbatim."; then
      return 1
    fi
  fi
  return 0
}

run_supervisor_agent() {
  local stage="$1"
  local message="$2"
  local context_file
  context_file=$(build_supervisor_context "$stage" "$message")
  local rc=0
  if ! run_runner_with_prompt "$context_file"; then
    rc=1
  fi
  rm -f "$context_file" 2>/dev/null || true
  return $rc
}

finalize_iteration() {
  local item_id="$1"
  local item_line="$2"
  local head_before="$3"

  LAST_FAILURE_STAGE=""
  LAST_FAILURE_MESSAGE=""

  local head_now
  head_now=$(git rev-parse HEAD 2>/dev/null || true)
  if [[ "$head_now" != "$head_before" ]]; then
    LAST_FAILURE_STAGE="mechanical"
    LAST_FAILURE_MESSAGE="Commit detected before controller finalize."
    return 1
  fi

  local moved_ids
  if ! moved_ids="$(move_checked_queue_items)"; then
    LAST_FAILURE_STAGE="pin-ops"
    LAST_FAILURE_MESSAGE="Failed to move checked queue items."
    return 1
  fi

  local first_item_after
  first_item_after=$(first_unchecked_queue_item)
  local first_item_after_id
  first_item_after_id=$(extract_item_id "$first_item_after")

  local completed=0
  if [[ -z "$first_item_after_id" || "$first_item_after_id" != "$item_id" ]]; then
    completed=1
  fi

  local dirty
  dirty=$(git status --porcelain)

  if [[ "$completed" -eq 0 ]]; then
    if [[ -n "$dirty" ]]; then
      LAST_FAILURE_STAGE="incomplete"
      LAST_FAILURE_MESSAGE="Working tree changed but ${item_id} was not marked complete."
      return 1
    fi
    if ! run_validate_pin; then
      LAST_FAILURE_STAGE="pin-validate"
      LAST_FAILURE_MESSAGE="validate_pin failed after iteration."
      return 1
    fi
    return 0
  fi

  if [[ -z "$dirty" ]]; then
    LAST_FAILURE_STAGE="complete"
    LAST_FAILURE_MESSAGE="Queue head moved but no changes detected."
    return 1
  fi

  local only_specs=1
  while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    if [[ "$path" != ralph_legacy/specs/* ]]; then
      only_specs=0
      break
    fi
  done < <(git diff --name-only)

  if [[ "$only_specs" -eq 0 ]]; then
    if ! run_make_ci; then
      LAST_FAILURE_STAGE="verify"
      LAST_FAILURE_MESSAGE="make ci failed."
      return 1
    fi
  fi

  local short_title
  short_title=$(extract_item_title "$item_line")
  if [[ -z "$short_title" ]]; then
    short_title="completed"
  fi

  git add -A
  git commit -m "${item_id}: ${short_title}"
  push_if_ahead

  if ! run_validate_pin; then
    LAST_FAILURE_STAGE="pin-validate"
    LAST_FAILURE_MESSAGE="validate_pin failed after commit."
    return 1
  fi

  return 0
}

handle_iteration_failure() {
  local stage="$1"
  local message="$2"

  echo ">> [RALPH] Iteration failure (${stage}): ${message}" >&2

  local attempt=1
  while [[ "$attempt" -le "$MAX_REPAIR_ATTEMPTS" ]]; do
    echo ">> [RALPH] Supervisor attempt ${attempt}/${MAX_REPAIR_ATTEMPTS}..." >&2
    if run_supervisor_agent "$stage" "$message"; then
      if finalize_iteration "$first_item_id" "$first_item" "$head_before"; then
        cleanup_iteration_artifacts
        return 0
      fi
      stage="$LAST_FAILURE_STAGE"
      message="$LAST_FAILURE_MESSAGE"
    else
      stage="supervisor"
      message="Supervisor runner failed."
    fi
    attempt=$((attempt + 1))
  done

  local wip_branch
  wip_branch=$(quarantine_current_state "$first_item_id" "$head_before" "$message")
  auto_block_item "$first_item_id" "$message" "$wip_branch" "$head_before"
  stalled=0
  cleanup_iteration_artifacts
  return 0
}

reconcile_checked_queue_items() {
  local moved_ids
  if ! moved_ids="$(move_checked_queue_items)"; then
    return 1
  fi
  if [[ -n "$moved_ids" ]]; then
    if ! git diff --quiet -- "$plan_path" "$done_path"; then
      git add "$plan_path" "$done_path"
      git commit -m "chore: move completed queue items (${moved_ids})"
      push_if_ahead
    fi
  fi
}

iterations=0
stalled=0

while true; do
  first_item="$(first_unchecked_queue_item)"
  if [[ -z "$first_item" ]]; then
    if [[ -n "$ONLY_TAGS" ]]; then
      echo ">> [RALPH] No unchecked items found in Queue for tags: $ONLY_TAGS. Exiting cleanly."
    else
      echo ">> [RALPH] No unchecked items found in Queue. Exiting cleanly."
    fi
    if [[ "$PUSH_FAILED" -eq 1 ]]; then
      ahead=$(git rev-list --count @{u}..HEAD 2>/dev/null || echo 0)
      echo ">> [RALPH] Push required; local branch ahead by ${ahead} commit(s)." >&2
    fi
    exit 0
  fi

  first_item_id="$(extract_item_id "$first_item")"
  if [[ -z "$first_item_id" ]]; then
    die "Queue item is missing an ID prefix (expected something like ABC-0123)."
  fi

  effort="low"
  if printf '%s' "$first_item" | grep -q '\[P1\]'; then
    effort="high"
  fi
  CURRENT_RUN_ARGS=("${RUNNER_ARGS[@]}")
  if [[ "$RUNNER" == "codex" ]]; then
    if ! printf '%s\n' "${CURRENT_RUN_ARGS[@]}" | grep -q "model_reasoning_effort"; then
      CURRENT_RUN_ARGS=(-c "model_reasoning_effort=\"${effort}\"" "${CURRENT_RUN_ARGS[@]}")
    fi
    EFFECTIVE_CODEX_EFFORT="$(detect_effective_codex_effort "$effort")"
    CONTEXT_BUILDER_MANDATORY=0
    if [[ "$EFFECTIVE_CODEX_EFFORT" == "low" || "$EFFECTIVE_CODEX_EFFORT" == "off" ]]; then
      CONTEXT_BUILDER_MANDATORY=1
    fi
  else
    EFFECTIVE_CODEX_EFFORT=""
    CONTEXT_BUILDER_MANDATORY=0
  fi

  head_before=$(git rev-parse HEAD 2>/dev/null || true)
  CURRENT_ITEM_BLOCK="$(current_item_block)"

  if ! reconcile_checked_queue_items; then
    handle_iteration_failure "pin-ops" "Failed to move checked queue items."
    continue
  fi

  head_before=$(git rev-parse HEAD 2>/dev/null || true)

  if [[ -n "$(git status --porcelain)" ]]; then
    handle_iteration_failure "preflight" "Working tree is dirty before iteration ${iterations}."
    continue
  fi

  if ! run_validate_pin; then
    handle_iteration_failure "pin-validate" "validate_pin failed before iteration ${iterations}."
    continue
  fi

  iterations=$((iterations + 1))
  echo ">> [RALPH] Iteration ${iterations}"

  tmp_prompt="$(mktemp)"
  {
    cat "$PROMPT_PATH"
    echo
    if [[ "$RUNNER" == "codex" && -n "${EFFECTIVE_CODEX_EFFORT}" ]]; then
      write_codex_context_builder_policy_block "$EFFECTIVE_CODEX_EFFORT" "$CONTEXT_BUILDER_MANDATORY"
      echo
    fi
    echo "# CURRENT QUEUE ITEM"
    printf '%s\n' "$CURRENT_ITEM_BLOCK"
  } > "$tmp_prompt"

  if ! run_runner_with_prompt "$tmp_prompt"; then
    rm -f "$tmp_prompt" 2>/dev/null || true
    handle_iteration_failure "runner" "${RUNNER} failed on iteration ${iterations}."
    continue
  fi
  rm -f "$tmp_prompt" 2>/dev/null || true

  if ! finalize_iteration "$first_item_id" "$first_item" "$head_before"; then
    handle_iteration_failure "$LAST_FAILURE_STAGE" "$LAST_FAILURE_MESSAGE"
    continue
  fi

  if [[ -n "$(git status --porcelain)" ]]; then
    handle_iteration_failure "post-commit" "Working tree is dirty after iteration ${iterations}."
    continue
  fi

  cleanup_iteration_artifacts

  first_item_after="$(first_unchecked_queue_item)"
  head_after=$(git rev-parse HEAD 2>/dev/null || true)
  if [[ "$head_before" == "$head_after" && "$first_item" == "$first_item_after" ]]; then
    stalled=$((stalled + 1))
  else
    stalled=0
  fi

  if [[ "$MAX_STALLED" -gt 0 && "$stalled" -ge "$MAX_STALLED" ]]; then
    handle_iteration_failure "stall" "Stalled for ${stalled} iterations (head and first queue item unchanged)."
    continue
  fi

  if [[ "$RUN_ONCE" -eq 1 ]]; then
    break
  fi

  if [[ "$MAX_ITERATIONS" -gt 0 && "$iterations" -ge "$MAX_ITERATIONS" ]]; then
    break
  fi

  if [[ "$SLEEP_SECS" -gt 0 ]]; then
    sleep "$SLEEP_SECS"
  fi
done

if [[ "$PUSH_FAILED" -eq 1 ]]; then
  ahead=$(git rev-list --count @{u}..HEAD 2>/dev/null || echo 0)
  echo ">> [RALPH] Push required; local branch ahead by ${ahead} commit(s)." >&2
fi
