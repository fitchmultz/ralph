#!/usr/bin/env bash
# runner_cli_inventory.sh
#
# Responsible for capturing `--help` (and best-effort `--version`) output for the
# runner CLIs Ralph uses: `codex`, `opencode`, `gemini`, `claude`, `kimi`, `pi`, and Cursor's
# `agent`. Outputs are written into a stable directory structure to support
# Phase 2 discovery and later Phase 3 runner unification work.
#
# Subcommand discovery:
# - After capturing base `--help`, the script attempts to parse subcommands from the output
# - It then runs `<runner> <subcommand> --help` for each discovered subcommand
# - This process is generic and works for all runners without hardcoded lists
#
# Does NOT:
# - Validate that a runner is correctly configured/authenticated
# - Execute any runner workloads beyond help/version commands
# - Parse or interpret help text beyond extracting subcommand names
#
# Assumptions / invariants:
# - Runner binaries are either on PATH or provided via `--bin NAME=PATH`
# - Captured help/version output may be written to stdout or stderr; we capture
#   both (`2>&1`)
# - The script is dependency-free beyond bash + coreutils

set -euo pipefail

usage() {
  cat <<'EOF'
runner_cli_inventory.sh

Capture `--help` (and best-effort `--version`) outputs for runner binaries used by Ralph.
Automatically discovers and captures subcommand help as well.

USAGE:
  scripts/runner_cli_inventory.sh [--out DIR] [--bin NAME=PATH]...

OPTIONS:
  --out DIR
      Output directory for captured help/version text.
      Default: target/tmp/runner_cli_inventory

  --bin NAME=PATH
      Override a runner binary name/path.
      May be provided multiple times.

      NAME must be one of: codex, opencode, gemini, claude, kimi, pi, agent

  -h, --help
      Print this help.

OUTPUT:
  Writes per-runner files under:
    <out>/<runner>/

  Including:
    resolved_path.txt
    version.txt (best effort)
    help.base.txt (main --help output)
    help.<subcommand>.txt (one per discovered subcommand)
    <runner>.md (consolidated file with all of the above)

EXAMPLES:
  scripts/runner_cli_inventory.sh
  scripts/runner_cli_inventory.sh --out target/tmp/runner_cli_inventory
  scripts/runner_cli_inventory.sh --bin agent=/Applications/Cursor.app/Contents/Resources/app/bin/agent
EOF
}

OUT_DIR="target/tmp/runner_cli_inventory"

BIN_OVERRIDE_CODEX=""
BIN_OVERRIDE_OPENCODE=""
BIN_OVERRIDE_GEMINI=""
BIN_OVERRIDE_CLAUDE=""
BIN_OVERRIDE_AGENT=""
BIN_OVERRIDE_KIMI=""
BIN_OVERRIDE_PI=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --bin)
      kv="${2:-}"
      if [[ -z "$kv" || "$kv" != *"="* ]]; then
        echo "ERROR: --bin requires NAME=PATH (got: '$kv')" >&2
        exit 2
      fi
      name="${kv%%=*}"
      path="${kv#*=}"
      case "$name" in
        codex|opencode|gemini|claude|kimi|pi|agent) ;;
        *)
          echo "ERROR: --bin NAME must be one of: codex, opencode, gemini, claude, kimi, pi, agent (got: '$name')" >&2
          exit 2
          ;;
      esac
      case "$name" in
        codex) BIN_OVERRIDE_CODEX="$path" ;;
        opencode) BIN_OVERRIDE_OPENCODE="$path" ;;
        gemini) BIN_OVERRIDE_GEMINI="$path" ;;
        claude) BIN_OVERRIDE_CLAUDE="$path" ;;
        agent) BIN_OVERRIDE_AGENT="$path" ;;
        kimi) BIN_OVERRIDE_KIMI="$path" ;;
        pi) BIN_OVERRIDE_PI="$path" ;;
      esac
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      echo >&2
      usage >&2
      exit 2
      ;;
  esac
done

mkdir -p "$OUT_DIR"

run_and_capture() {
  local runner="$1"
  local label="$2"
  shift 2
  local dir="$OUT_DIR/$runner"
  local file="$dir/help.${label}.txt"

  mkdir -p "$dir"

  (
    set +e
    echo "=== runner: $runner"
    echo "=== label: $label"
    echo "=== cmd: $*"
    echo "=== captured_at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo
    "$@" < /dev/null 2>&1
    cmd_status=$?
    echo
    if [[ "$cmd_status" -ne 0 ]]; then
      echo "=== ERROR: command failed (exit=$cmd_status)"
    fi
    exit "$cmd_status"
  ) > "$file"
}

capture_version_best_effort() {
  local runner="$1"
  local bin="$2"
  local dir="$OUT_DIR/$runner"
  local file="$dir/version.txt"
  mkdir -p "$dir"

  # Try a handful of common version invocations; write the first that succeeds.
  local candidates=(
    "--version"
    "version"
    "-V"
  )

  {
    echo "=== runner: $runner"
    echo "=== bin: $bin"
    echo "=== captured_at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo
  } > "$file"

  for arg in "${candidates[@]}"; do
    if "$bin" $arg >/dev/null 2>&1; then
      {
        echo "=== cmd: $bin $arg"
        "$bin" $arg 2>&1
      } >> "$file"
      return 0
    fi
  done

  {
    echo "=== WARNING: no supported version flag detected (tried: ${candidates[*]})"
    echo "=== NOTE: this is not fatal; rely on help text headers if they include versions."
  } >> "$file"
  return 0
}

resolve_bin() {
  local runner="$1"
  case "$runner" in
    codex)
      if [[ -n "$BIN_OVERRIDE_CODEX" ]]; then
        echo "$BIN_OVERRIDE_CODEX"
        return 0
      fi
      ;;
    opencode)
      if [[ -n "$BIN_OVERRIDE_OPENCODE" ]]; then
        echo "$BIN_OVERRIDE_OPENCODE"
        return 0
      fi
      ;;
    gemini)
      if [[ -n "$BIN_OVERRIDE_GEMINI" ]]; then
        echo "$BIN_OVERRIDE_GEMINI"
        return 0
      fi
      ;;
    claude)
      if [[ -n "$BIN_OVERRIDE_CLAUDE" ]]; then
        echo "$BIN_OVERRIDE_CLAUDE"
        return 0
      fi
      ;;
    agent)
      if [[ -n "$BIN_OVERRIDE_AGENT" ]]; then
        echo "$BIN_OVERRIDE_AGENT"
        return 0
      fi
      ;;
    kimi)
      if [[ -n "$BIN_OVERRIDE_KIMI" ]]; then
        echo "$BIN_OVERRIDE_KIMI"
        return 0
      fi
      ;;
    pi)
      if [[ -n "$BIN_OVERRIDE_PI" ]]; then
        echo "$BIN_OVERRIDE_PI"
        return 0
      fi
      ;;
  esac
  if command -v "$runner" >/dev/null 2>&1; then
    command -v "$runner"
    return 0
  fi
  # Fall back to raw name; capture attempt will fail and be recorded.
  echo "$runner"
}

write_resolved_path() {
  local runner="$1"
  local bin="$2"
  local dir="$OUT_DIR/$runner"
  mkdir -p "$dir"
  {
    echo "=== runner: $runner"
    echo "=== resolved_bin: $bin"
    echo "=== captured_at: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  } > "$dir/resolved_path.txt"
}

# Extract subcommands from help output.
# Reads from stdin (help text) and outputs one subcommand per line.
# Handles multiple common CLI help formats.
extract_subcommands() {
  local in_commands_section=0
  local line

  while IFS= read -r line; do
    # Skip metadata lines (lines starting with === which we add to captured output)
    if [[ "$line" =~ ^=== ]]; then
      continue
    fi

    # Check for section headers that indicate command listings
    # Patterns: "Commands:", "Commands ─", "COMMANDS", "Subcommands:"
    if [[ "$line" =~ ^([Cc]ommands?|COMMANDS|SUBCOMMANDS?)[[:space:]:─] ]]; then
      in_commands_section=1
      continue
    fi

    # Rich box-drawing format section header (╭─ Commands ───╮)
    if [[ "$line" =~ ^╭.*[Cc]ommands?.*─*╮ ]]; then
      in_commands_section=1
      continue
    fi

    # Check for section enders
    if [[ $in_commands_section -eq 1 ]]; then
      # End of commands section detection - these headers indicate end of commands
      if [[ "$line" =~ ^([Oo]ptions?|OPTIONS|ARGUMENTS|Positionals|POSITIONALS|Global[[:space:]]options|GLOBAL[[:space:]]OPTIONS|Flags?): ]]; then
        # Next section started
        in_commands_section=0
        continue
      elif [[ "$line" =~ ^╭.* ]]; then
        # Any new rich box section ends the commands section
        in_commands_section=0
        continue
      fi
    fi

    if [[ $in_commands_section -eq 1 ]]; then
      local cmd=""

      # Pattern 1: Box-drawing format like "│ login    Description... │"
      # Skip header/footer lines
      if [[ "$line" =~ ^[[:space:]]*╭ ]] || [[ "$line" =~ ^[[:space:]]*╰ ]]; then
        continue
      fi
      # Match lines with box drawing characters
      if [[ "$line" =~ [│┃] ]]; then
        # Extract content between vertical bars
        local content
        content=$(echo "$line" | sed -E 's/^[[:space:]]*[│┃][[:space:]]*//; s/[[:space:]]*[│┃][[:space:]]*$//')
        # First word is the command
        cmd=$(echo "$content" | awk '{print $1}')
      fi

      # Pattern 2: Standard format like "  command    description"
      # or "  opencode command    description" (where command is the actual subcommand)
      # Only match lines starting with exactly 2 spaces (not more, which indicates wrapped text)
      if [[ -z "$cmd" ]]; then
        # First, try to match "  toolname command  description" format
        # The command is followed by either: space(s) then description, bracket/angle bracket (for args), or end of line
        if [[ "$line" =~ ^[[:space:]]{2}(opencode|gemini|codex|claude|kimi|pi|agent)[[:space:]]+([a-z][a-z0-9_-]*)[[:space:]]+ ]]; then
          # Second word is the actual command (extract before any brackets)
          local second_word="${BASH_REMATCH[2]}"
          cmd="${second_word%%[\[\<\|]*}"
        # Then try "  command    description" format (at least 2 spaces before description)
        elif [[ "$line" =~ ^[[:space:]]{2}([a-z][a-z0-9_-]*)[[:space:]]{2,} ]]; then
          cmd="${BASH_REMATCH[1]}"
        fi
      fi

      # Validate the command looks reasonable
      if [[ -n "$cmd" ]]; then
        # Skip common false positives and metadata patterns
        case "$cmd" in
          -h|--help|--version|-V|Usage|usage|Options|options|Commands|commands|Arguments|arguments)
            continue
            ;;
        esac
        # Skip lines that look like timestamps or metadata
        if [[ "$cmd" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T ]]; then
          continue
        fi
        # Only output valid-looking commands (lowercase, hyphens, numbers)
        if [[ "$cmd" =~ ^[a-z][a-z0-9_-]*$ ]]; then
          echo "$cmd"
        fi
      fi
    fi
  done | sort -u
}

# Capture help for all discovered subcommands
capture_subcommand_helps() {
  local runner="$1"
  local bin="$2"
  local help_file="$OUT_DIR/$runner/help.base.txt"

  if [[ ! -f "$help_file" ]]; then
    return 0
  fi

  echo "    discovering subcommands from help.base.txt..."

  # Extract subcommands from the captured help, filtering out the runner name
  local subcommands
  subcommands=$(extract_subcommands < "$help_file" | grep -v "^${runner}$" || true)

  if [[ -z "$subcommands" ]]; then
    echo "    no subcommands discovered"
    return 0
  fi

  local count=0
  while IFS= read -r subcmd; do
    # Skip empty lines
    [[ -z "$subcmd" ]] && continue

    echo "    capturing: $subcmd --help"
    run_and_capture "$runner" "$subcmd" "$bin" "$subcmd" "--help" || true
    count=$((count + 1))
  done < <(echo "$subcommands")

  echo "    captured $count subcommand(s)"
}

# Create a consolidated markdown file with all captured info for a runner
create_consolidated_file() {
  local runner="$1"
  local runner_dir="$OUT_DIR/$runner"
  local consolidated_file="$runner_dir/${runner}.md"

  if [[ ! -d "$runner_dir" ]]; then
    return 0
  fi

  {
    echo "# ${runner} CLI Inventory"
    echo ""
    echo "Generated: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    echo ""

    # Section 1: Resolved path
    if [[ -f "$runner_dir/resolved_path.txt" ]]; then
      echo "## Binary Path"
      echo ""
      echo "\`\`\`"
      cat "$runner_dir/resolved_path.txt"
      echo "\`\`\`"
      echo ""
    fi

    # Section 2: Version
    if [[ -f "$runner_dir/version.txt" ]]; then
      echo "## Version"
      echo ""
      echo "\`\`\`"
      cat "$runner_dir/version.txt"
      echo "\`\`\`"
      echo ""
    fi

    # Section 3: Base help
    if [[ -f "$runner_dir/help.base.txt" ]]; then
      echo "## Base Help (--help)"
      echo ""
      echo "\`\`\`"
      cat "$runner_dir/help.base.txt"
      echo "\`\`\`"
      echo ""
    fi

    # Section 4: Subcommand helps
    local subcmd_files
    subcmd_files=$(find "$runner_dir" -name 'help.*.txt' ! -name 'help.base.txt' | sort)
    if [[ -n "$subcmd_files" ]]; then
      echo "## Subcommand Helps"
      echo ""
      while IFS= read -r subcmd_file; do
        local subcmd_name
        subcmd_name=$(basename "$subcmd_file" .txt | sed 's/help\.//')
        echo "### ${subcmd_name}"
        echo ""
        echo "\`\`\`"
        cat "$subcmd_file"
        echo "\`\`\`"
        echo ""
      done <<< "$subcmd_files"
    fi

  } > "$consolidated_file"

  echo "    consolidated: ${runner}.md"
}

declare -a RUNNERS=("codex" "opencode" "gemini" "claude" "kimi" "pi" "agent")

echo "Runner CLI inventory: start"
echo "Output dir: $OUT_DIR"
echo

failures=0

for runner in "${RUNNERS[@]}"; do
  bin="$(resolve_bin "$runner")"
  write_resolved_path "$runner" "$bin"

  echo "==> $runner"
  echo "    bin: $bin"

  # Version capture is best-effort and never fatal.
  capture_version_best_effort "$runner" "$bin" || true

  # Always capture base help (fatal only if we cannot execute at all).
  if ! run_and_capture "$runner" "base" "$bin" "--help"; then
    echo "    ERROR: failed to run '$bin --help' (see $OUT_DIR/$runner/help.base.txt)" >&2
    failures=$((failures + 1))
    echo
    continue
  fi

  # Discover and capture subcommand helps from the base help output
  capture_subcommand_helps "$runner" "$bin"

  # Create consolidated file
  create_consolidated_file "$runner"

  echo "    captured: $OUT_DIR/$runner/"
  echo
done

echo "Runner CLI inventory: complete"
if [[ "$failures" -gt 0 ]]; then
  echo "WARNING: $failures runner(s) failed base --help capture. See files under: $OUT_DIR" >&2
  exit 1
fi
