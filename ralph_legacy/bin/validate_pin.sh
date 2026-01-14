#!/usr/bin/env bash
# Purpose: Validate Ralph pin/spec files for structural and ID integrity.
# Entrypoint: validate_pin.sh

set -euo pipefail

die() {
  printf 'Error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'USAGE'
Validate Ralph pin/spec files for required sections and unique queue IDs.

Additionally, this validates that any top-level items in `## Queue` follow the
required format, including `Evidence:` and `Plan:` sub-bullets.

Usage:
  ralph_legacy/bin/validate_pin.sh [--help]

Examples:
  ralph_legacy/bin/validate_pin.sh
  ralph_legacy/bin/validate_pin.sh --help

Required Queue item format (template):
  - [ ] RQ-0135 [code]: Short actionable title. (path/to/file.py, Makefile)
    - Evidence: Concrete failure evidence (command/output/traceback/etc.).
    - Plan: Concise plan of attack.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
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

queue_path="${repo_root}/ralph_legacy/specs/implementation_queue.md"
done_path="${repo_root}/ralph_legacy/specs/implementation_done.md"
lookup_path="${repo_root}/ralph_legacy/specs/lookup_table.md"
readme_path="${repo_root}/ralph_legacy/specs/README.md"
prompt_path="${repo_root}/ralph_legacy/prompt.md"

[[ -f "$queue_path" ]] || die "Missing $queue_path"
[[ -f "$done_path" ]] || die "Missing $done_path"
[[ -f "$lookup_path" ]] || die "Missing $lookup_path"
[[ -f "$readme_path" ]] || die "Missing $readme_path"
[[ -f "$prompt_path" ]] || die "Missing $prompt_path"

# Ensure required sections exist.
queue_sections="$(grep -nE '^## (Queue|Blocked|Parking Lot)$' "$queue_path" || true)"
printf '%s\n' "$queue_sections" | grep -q '## Queue' || die "Queue file missing '## Queue'"
printf '%s\n' "$queue_sections" | grep -q '## Blocked' || die "Queue file missing '## Blocked'"
printf '%s\n' "$queue_sections" | grep -q '## Parking Lot' || die "Queue file missing '## Parking Lot'"

# Extract top-level task IDs only (not references inside plans).
extract_ids() {
  local path="$1"
  awk '
    $0 ~ /^- \[[ x]\] / {
      if (match($0, /[A-Z0-9]{2,10}-[0-9]{4}/)) {
        print substr($0, RSTART, RLENGTH)
      }
    }
  ' "$path"
}

ids="$(
  {
    extract_ids "$queue_path"
    extract_ids "$done_path"
  } | sort
)"

if [[ -z "$ids" ]]; then
  die "No task IDs found in queue/done. Expected IDs like RQ-0123."
fi

dupes="$(printf '%s\n' "$ids" | uniq -d)"
if [[ -n "$dupes" ]]; then
  die "Duplicate task IDs detected. Fix these IDs:\n$dupes"
fi

# Ensure every top-level queue task line has an ID.
missing_id_lines="$(
  awk '
    $0 ~ /^- \[[ x]\] / {
      if ($0 !~ /[A-Z0-9]{2,10}-[0-9]{4}/) {
        print
      }
    }
  ' "$queue_path"
)"
if [[ -n "$missing_id_lines" ]]; then
  die "Queue has top-level items missing an ID:\n$missing_id_lines"
fi

# Enforce required Queue item structure for items in the `## Queue` section.
# This is intentionally stricter than the done log; it is meant to prevent new
# misformatted items from entering the executable queue.
bad_queue_format="$(
  awk '
    BEGIN {
      in_queue = 0
      item_active = 0
      bad = 0
      header = ""
      n = 0
    }

    function start_item(line) {
      header = line
      delete lines
      n = 0
      item_active = 1
    }

    function add_line(line) {
      n += 1
      lines[n] = line
    }

    function finish_item(   id_ok, tag_ok, colon_ok, scope_ok, evidence_ok, plan_ok, i) {
      if (!item_active) return

      id_ok = (header ~ /[A-Z0-9]{2,10}-[0-9]{4}/)
      tag_ok = (header ~ /\[(db|ui|code|ops|docs)\]/)
      colon_ok = (header ~ /:[[:space:]]/)
      scope_ok = (header ~ /\([^()]+\)[[:space:]]*$/)

      evidence_ok = 0
      plan_ok = 0
      for (i = 1; i <= n; i++) {
        if (lines[i] ~ /^[[:space:]]+- Evidence:/) evidence_ok = 1
        if (lines[i] ~ /^[[:space:]]+- Plan:/) plan_ok = 1
      }

      if (!(id_ok && tag_ok && colon_ok && scope_ok && evidence_ok && plan_ok)) {
        bad = 1
        print "Bad queue item format:"
        print header
        if (!id_ok) print "  - Missing ID like RQ-0123"
        if (!tag_ok) print "  - Missing routing tag like [code]/[db]/[ui]/[ops]/[docs]"
        if (!colon_ok) print "  - Missing \":\" after ID/tags"
        if (!scope_ok) print "  - Missing trailing scope list in parentheses, e.g. (path/to/file.py, Makefile)"
        if (!evidence_ok) print "  - Missing indented metadata bullet: \"- Evidence: ...\""
        if (!plan_ok) print "  - Missing indented metadata bullet: \"- Plan: ...\""
        print ""
      }

      header = ""
      delete lines
      n = 0
      item_active = 0
    }

    /^## Queue[[:space:]]*$/ {
      finish_item()
      in_queue = 1
      next
    }

    /^## / {
      if (in_queue) {
        finish_item()
      }
      in_queue = 0
      next
    }

    in_queue {
      if ($0 ~ /^- \[[ x]\] /) {
        finish_item()
        start_item($0)
        next
      }
      if (item_active) {
        add_line($0)
      }
      next
    }

    END {
      finish_item()
      exit bad
    }
  ' "$queue_path"
)"
if [[ -n "$bad_queue_format" ]]; then
  die "Queue items in ## Queue must follow the required format (ID + routing tag(s) + scope + Evidence + Plan).\n\n${bad_queue_format}"
fi

echo ">> [RALPH] Pin validation OK."
