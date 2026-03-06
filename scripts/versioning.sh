#!/usr/bin/env bash
#
# Purpose: Manage Ralph's canonical release version and sync derived metadata.
# Responsibilities:
# - Read the repo-wide semantic version from VERSION.
# - Keep Cargo, Xcode, and macOS CLI compatibility metadata synchronized.
# - Validate that checked-in version metadata has not drifted.
# Scope:
# - Version metadata only; does not build, test, tag, or publish releases.
# Usage:
# - scripts/versioning.sh current
# - scripts/versioning.sh check
# - scripts/versioning.sh sync [--version x.y.z]
# Invariants/assumptions:
# - VERSION is the canonical semantic version for the repo.
# - Cargo.toml, Xcode project settings, and VersionValidator.swift are generated from VERSION.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION_FILE="$REPO_ROOT/VERSION"
CARGO_TOML="$REPO_ROOT/crates/ralph/Cargo.toml"
XCODE_PROJECT="$REPO_ROOT/apps/RalphMac/RalphMac.xcodeproj/project.pbxproj"
VERSION_VALIDATOR_SWIFT="$REPO_ROOT/apps/RalphMac/RalphCore/VersionValidator.swift"

log_error() {
    echo "versioning: $*" >&2
}

validate_semver() {
    local version="$1"
    if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        log_error "invalid semantic version: $version"
        return 1
    fi
}

read_canonical_version() {
    if [ ! -f "$VERSION_FILE" ]; then
        log_error "missing VERSION file: $VERSION_FILE"
        return 1
    fi

    local version
    version=$(tr -d '[:space:]' < "$VERSION_FILE")
    validate_semver "$version"
    printf '%s\n' "$version"
}

version_to_build_number() {
    local version="$1"
    local major minor patch
    IFS='.' read -r major minor patch <<< "$version"
    printf '%d\n' "$((major * 10000 + minor * 100 + patch))"
}

sync_version_metadata() {
    local version="$1"
    local build_number
    build_number=$(version_to_build_number "$version")

    printf '%s\n' "$version" > "$VERSION_FILE"

    python3 - "$version" "$build_number" "$CARGO_TOML" "$XCODE_PROJECT" "$VERSION_VALIDATOR_SWIFT" <<'PY'
from pathlib import Path
import re
import sys

version, build_number, cargo_toml, xcode_project, version_validator = sys.argv[1:6]

def replace_once(path_str: str, pattern: str, replacement: str) -> None:
    path = Path(path_str)
    text = path.read_text(encoding="utf-8")
    updated, count = re.subn(pattern, replacement, text, count=0, flags=re.MULTILINE)
    if count == 0:
        raise SystemExit(f"versioning: failed to update {path}: pattern {pattern!r} not found")
    path.write_text(updated, encoding="utf-8")

replace_once(
    cargo_toml,
    r'(?m)^version = "[0-9]+\.[0-9]+\.[0-9]+"$',
    f'version = "{version}"',
)
replace_once(
    xcode_project,
    r'(?m)^(\s*)MARKETING_VERSION = [0-9]+\.[0-9]+\.[0-9]+;$',
    rf'\1MARKETING_VERSION = {version};',
)
replace_once(
    xcode_project,
    r'(?m)^(\s*)CURRENT_PROJECT_VERSION = [0-9]+;$',
    rf'\1CURRENT_PROJECT_VERSION = {build_number};',
)
replace_once(
    version_validator,
    r'(?m)^(\s*public static let minimumCLIVersion = )"[0-9]+\.[0-9]+\.[0-9]+"$',
    rf'\1"{version}"',
)
replace_once(
    version_validator,
    r'(?m)^(\s*public static let maximumCLIVersion = )"[0-9]+\.[0-9]+\.[0-9]+"$',
    rf'\1"{version}"',
)
PY
}

get_first_match() {
    local pattern="$1"
    local path="$2"
    rg -o --replace '$1' "$pattern" "$path" | head -1
}

check_version_metadata() {
    local version="$1"
    local build_number="$2"
    local cargo_version marketing_version current_project_version minimum_cli maximum_cli

    cargo_version=$(get_first_match '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$' "$CARGO_TOML")
    marketing_version=$(get_first_match 'MARKETING_VERSION = ([0-9]+\.[0-9]+\.[0-9]+);' "$XCODE_PROJECT")
    current_project_version=$(get_first_match 'CURRENT_PROJECT_VERSION = ([0-9]+);' "$XCODE_PROJECT")
    minimum_cli=$(get_first_match 'minimumCLIVersion = "([0-9]+\.[0-9]+\.[0-9]+)"' "$VERSION_VALIDATOR_SWIFT")
    maximum_cli=$(get_first_match 'maximumCLIVersion = "([0-9]+\.[0-9]+\.[0-9]+)"' "$VERSION_VALIDATOR_SWIFT")

    local failures=0

    if [ "$cargo_version" != "$version" ]; then
        log_error "Cargo.toml version drifted: expected $version, found ${cargo_version:-<missing>}"
        failures=1
    fi
    if [ "$marketing_version" != "$version" ]; then
        log_error "Xcode MARKETING_VERSION drifted: expected $version, found ${marketing_version:-<missing>}"
        failures=1
    fi
    if [ "$current_project_version" != "$build_number" ]; then
        log_error "Xcode CURRENT_PROJECT_VERSION drifted: expected $build_number, found ${current_project_version:-<missing>}"
        failures=1
    fi
    if [ "$minimum_cli" != "$version" ]; then
        log_error "VersionValidator minimumCLIVersion drifted: expected $version, found ${minimum_cli:-<missing>}"
        failures=1
    fi
    if [ "$maximum_cli" != "$version" ]; then
        log_error "VersionValidator maximumCLIVersion drifted: expected $version, found ${maximum_cli:-<missing>}"
        failures=1
    fi

    if [ "$failures" -ne 0 ]; then
        log_error "run: scripts/versioning.sh sync --version $version"
        return 1
    fi
}

print_usage() {
    cat <<'EOF'
Usage:
  scripts/versioning.sh current
  scripts/versioning.sh check
  scripts/versioning.sh sync [--version x.y.z]

Commands:
  current            Print the canonical repo version from VERSION
  check              Verify all derived version metadata matches VERSION
  sync               Rewrite derived version metadata from VERSION or --version

Options:
  --version x.y.z    Semantic version to write before syncing
  --help, -h         Show this help

Examples:
  scripts/versioning.sh current
  scripts/versioning.sh check
  scripts/versioning.sh sync --version 0.2.0

Exit codes:
  0  Success
  1  Runtime or unexpected failure
  2  Usage/validation error
EOF
}

main() {
    local command="${1:-}"
    local version_arg=""

    case "$command" in
        current|check|sync)
            shift
            ;;
        --help|-h|"")
            print_usage
            [ -n "$command" ] || exit 2
            exit 0
            ;;
        *)
            log_error "unknown command: $command"
            print_usage
            exit 2
            ;;
    esac

    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                version_arg="${2:-}"
                if [ -z "$version_arg" ]; then
                    log_error "--version requires a value"
                    exit 2
                fi
                validate_semver "$version_arg" || exit 2
                shift 2
                ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            *)
                log_error "unknown argument: $1"
                print_usage
                exit 2
                ;;
        esac
    done

    local version
    if [ -n "$version_arg" ]; then
        version="$version_arg"
    else
        version=$(read_canonical_version)
    fi

    local build_number
    build_number=$(version_to_build_number "$version")

    case "$command" in
        current)
            printf '%s\n' "$version"
            ;;
        check)
            check_version_metadata "$version" "$build_number"
            printf 'versioning: OK (%s, build %s)\n' "$version" "$build_number"
            ;;
        sync)
            sync_version_metadata "$version"
            check_version_metadata "$version" "$build_number"
            printf 'versioning: synced %s (build %s)\n' "$version" "$build_number"
            ;;
    esac
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
