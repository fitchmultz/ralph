#!/usr/bin/env bash
#
# Purpose: Verify Ralph's pinned Rust source-build baseline and rustup drift state.
# Responsibilities:
# - Read the repo-local Rust channel from rust-toolchain.toml.
# - Verify the CLI crate rust-version matches the pinned channel's major.minor baseline.
# - Verify local rustup/cargo/rustc commands resolve to the pinned repo toolchain.
# - Optionally fail when global rustup stable outside the repo override differs from the pinned channel.
# Scope:
# - Rust toolchain baseline only; release semver metadata remains owned by scripts/versioning.sh.
# Usage:
# - scripts/check-rust-toolchain.sh
# - scripts/check-rust-toolchain.sh --fail-on-global-stable-drift
# Invariants/assumptions:
# - Run from any location; the script resolves the repo root automatically.
# - rust-toolchain.toml is the source of truth for the repo-local Rust channel.
# - crates/ralph/Cargo.toml rust-version follows the same major.minor source-build baseline.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/ralph-shell.sh
source "$SCRIPT_DIR/lib/ralph-shell.sh"
REPO_ROOT="$(ralph_repo_root)"

FAIL_ON_GLOBAL_STABLE_DRIFT=0

usage() {
    cat <<'EOF'
Verify Ralph's Rust source-build toolchain baseline.

Usage:
  scripts/check-rust-toolchain.sh [OPTIONS]

Options:
  --fail-on-global-stable-drift
      Also fail when rustup global stable outside this repository differs from
      the repo-local rust-toolchain.toml channel.
  -h, --help
      Show this help message.

Examples:
  scripts/check-rust-toolchain.sh
  scripts/check-rust-toolchain.sh --fail-on-global-stable-drift
  make rust-toolchain-check
  make rust-toolchain-drift-check

Exit codes:
  0  Toolchain checks passed
  1  Toolchain drift or runtime failure detected
  2  Invalid usage
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --fail-on-global-stable-drift)
            FAIL_ON_GLOBAL_STABLE_DRIFT=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            ralph_log_error "unknown option: $1"
            usage >&2
            exit 2
            ;;
    esac
    shift
done

read_toml_string_value() {
    local file="$1"
    local key="$2"
    python3 - "$file" "$key" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
key = re.escape(sys.argv[2])
match = re.search(rf'^\s*{key}\s*=\s*"([^"]+)"\s*$', text, re.MULTILINE)
if not match:
    raise SystemExit(f"missing {sys.argv[2]} in {sys.argv[1]}")
print(match.group(1))
PY
}

channel_minor() {
    local channel="$1"
    case "$channel" in
        [0-9]*.[0-9]*.[0-9]*)
            printf '%s\n' "${channel%.*}"
            ;;
        *)
            ralph_log_error "rust-toolchain.toml channel must be an exact stable version like 1.95.0, found: $channel"
            return 1
            ;;
    esac
}

rust_version_number() {
    local output="$1"
    printf '%s\n' "$output" | sed -n 's/^rustc \([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | head -1
}

cargo_version_number() {
    local output="$1"
    printf '%s\n' "$output" | sed -n 's/^cargo \([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | head -1
}

require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        ralph_log_error "$command_name is required to verify Ralph's Rust toolchain"
        return 1
    fi
}

require_rustup_component() {
    local component="$1"
    local toolchain="$2"
    if ! rustup component list --toolchain "$toolchain" --installed 2>/dev/null | grep -E "^${component}(-|$)" >/dev/null; then
        ralph_log_error "pinned Rust toolchain $toolchain is missing required component: $component"
        ralph_log_error "run: rustup toolchain install $toolchain --component rustfmt --component clippy"
        return 1
    fi
}

compare_global_stable() {
    local pinned_channel="$1"
    local temp_dir
    temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/ralph-rust-toolchain.XXXXXX")"

    local stable_rustc_output
    stable_rustc_output="$(cd "$temp_dir" && rustup run stable rustc --version 2>/dev/null || true)"
    rm -rf "$temp_dir"
    if [ -z "$stable_rustc_output" ]; then
        ralph_log_error "global rustup stable toolchain is unavailable; run: rustup toolchain install stable"
        return 1
    fi

    local stable_version
    stable_version="$(rust_version_number "$stable_rustc_output")"
    if [ -z "$stable_version" ]; then
        ralph_log_error "unable to parse global stable rustc version from: $stable_rustc_output"
        return 1
    fi

    if [ "$stable_version" != "$pinned_channel" ]; then
        ralph_log_error "global rustup stable drift detected: repo pins $pinned_channel but global stable rustc is $stable_version"
        ralph_log_error "when intentionally adopting a new stable, update rust-toolchain.toml and crates/ralph/Cargo.toml rust-version together"
        return 1
    fi

    ralph_log_success "Global rustup stable matches repo-pinned Rust $pinned_channel"
}

main() {
    require_command python3
    require_command rustup
    require_command rustc
    require_command cargo

    local pinned_channel pinned_minor crate_rust_version
    pinned_channel="$(read_toml_string_value "$REPO_ROOT/rust-toolchain.toml" channel)"
    pinned_minor="$(channel_minor "$pinned_channel")"
    crate_rust_version="$(read_toml_string_value "$REPO_ROOT/crates/ralph/Cargo.toml" rust-version)"

    if [ "$crate_rust_version" != "$pinned_minor" ]; then
        ralph_log_error "crate rust-version drifted: expected $pinned_minor from rust-toolchain.toml $pinned_channel, found $crate_rust_version"
        return 1
    fi

    local pinned_rustc
    pinned_rustc="$(rustup which rustc --toolchain "$pinned_channel" 2>/dev/null || true)"
    if [ -z "$pinned_rustc" ]; then
        ralph_log_error "pinned Rust toolchain is not installed: $pinned_channel"
        ralph_log_error "run: rustup toolchain install $pinned_channel --component rustfmt --component clippy"
        return 1
    fi

    require_rustup_component rustfmt "$pinned_channel"
    require_rustup_component clippy "$pinned_channel"

    local repo_active repo_rustc_output repo_cargo_output repo_rustc_version repo_cargo_version
    repo_active="$(cd "$REPO_ROOT" && rustup show active-toolchain 2>/dev/null || true)"
    case "$repo_active" in
        "$pinned_channel"-*) ;;
        *)
            ralph_log_error "repo active toolchain drifted: expected $pinned_channel, found ${repo_active:-<missing>}"
            return 1
            ;;
    esac

    repo_rustc_output="$(cd "$REPO_ROOT" && rustc --version)"
    repo_rustc_version="$(rust_version_number "$repo_rustc_output")"
    if [ "$repo_rustc_version" != "$pinned_channel" ]; then
        ralph_log_error "repo rustc drifted: expected $pinned_channel, found ${repo_rustc_version:-$repo_rustc_output}"
        return 1
    fi

    repo_cargo_output="$(cd "$REPO_ROOT" && cargo --version)"
    repo_cargo_version="$(cargo_version_number "$repo_cargo_output")"
    if [ "$repo_cargo_version" != "$pinned_channel" ]; then
        ralph_log_error "repo cargo drifted: expected $pinned_channel, found ${repo_cargo_version:-$repo_cargo_output}"
        return 1
    fi

    ralph_log_success "Repo Rust baseline is internally consistent: rust-toolchain.toml $pinned_channel, crate rust-version $crate_rust_version"
    ralph_log_success "Repo-local rustup, rustc, cargo, rustfmt, and clippy resolve to pinned Rust $pinned_channel"

    if [ "$FAIL_ON_GLOBAL_STABLE_DRIFT" -eq 1 ]; then
        compare_global_stable "$pinned_channel"
    else
        ralph_log_info "Skipped global stable drift check; run with --fail-on-global-stable-drift for release/public readiness."
    fi
}

main "$@"
