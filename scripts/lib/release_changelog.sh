#!/usr/bin/env bash
#
# Purpose: Encapsulate changelog promotion and release-notes rendering for releases.
# Responsibilities:
# - Promote `CHANGELOG.md` Unreleased content into a tagged release section.
# - Render release notes from the template using changelog and checksum inputs.
# Scope:
# - Text transformation only; publishing is handled elsewhere.
# Usage:
# - source "$(dirname "$0")/lib/release_changelog.sh"
# Invariants/assumptions:
# - CHANGELOG.md follows Keep a Changelog-style section headers.
# - REPO_HTTP_URL is set before link rewriting.

if [ -n "${RALPH_RELEASE_CHANGELOG_SOURCED:-}" ]; then
    return 0
fi
RALPH_RELEASE_CHANGELOG_SOURCED=1

release_render_notes_template() {
    local template_path="$1"
    local output_path="$2"
    local version="$3"
    local changelog_path="$4"
    local checksums_path="$5"
    local repo_url="$6"

    python3 - "$template_path" "$output_path" "$version" "$changelog_path" "$checksums_path" "$repo_url" <<'PY'
from pathlib import Path
import sys

template_path, output_path, version, changelog_path, checksums_path, repo_url = sys.argv[1:7]
template = Path(template_path).read_text(encoding="utf-8")
changelog = Path(changelog_path).read_text(encoding="utf-8").rstrip("\n")
checksums = Path(checksums_path).read_text(encoding="utf-8").rstrip("\n")
rendered = (
    template.replace("{{VERSION}}", version)
    .replace("{{REPO_URL}}", repo_url)
    .replace("{{REPO_CLONE_URL}}", f"{repo_url}.git")
    .replace("{{CHANGELOG_SECTION}}", changelog)
    .replace("{{CHECKSUMS}}", checksums)
)
Path(output_path).write_text(rendered, encoding="utf-8")
PY
}

release_validate_changelog_shape() {
    local changelog="$1"
    local base_version
    base_version=$(sed -n -E 's|^\[Unreleased\]: .*compare/v([0-9]+\.[0-9]+\.[0-9]+)\.\.\.HEAD.*|\1|p' "$changelog" | head -1 || true)
    if [ -z "$base_version" ]; then
        base_version=$(sed -n -E 's|^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*|\1|p' "$changelog" | head -1 || true)
    fi
    [ -n "$base_version" ]
}

release_promote_changelog() {
    local changelog="$1"
    local version="$2"
    local today="$3"
    local temp_file
    temp_file=$(ralph_mktemp_file "ralph-release-changelog")

    local unreleased_base_version
    unreleased_base_version=$(sed -n -E 's|^\[Unreleased\]: .*compare/v([0-9]+\.[0-9]+\.[0-9]+)\.\.\.HEAD.*|\1|p' "$changelog" | head -1 || true)
    if [ -z "$unreleased_base_version" ]; then
        unreleased_base_version=$(sed -n -E 's|^## \[([0-9]+\.[0-9]+\.[0-9]+)\].*|\1|p' "$changelog" | head -1 || true)
    fi
    if [ -z "$unreleased_base_version" ]; then
        ralph_log_error "Could not determine previous release version from CHANGELOG.md"
        rm -f "$temp_file"
        return 1
    fi

    local in_unreleased=0
    local found_unreleased=0
    local unreleased_content=""
    local before_unreleased=""
    local after_unreleased=""
    local line

    while IFS= read -r line || [ -n "$line" ]; do
        if [ "$found_unreleased" -eq 0 ]; then
            if [[ "$line" =~ ^##\ \[Unreleased\] ]]; then
                found_unreleased=1
                in_unreleased=1
            else
                before_unreleased="${before_unreleased}${line}"$'\n'
            fi
        elif [ "$in_unreleased" -eq 1 ]; then
            if [[ "$line" =~ ^##\ \[ ]]; then
                in_unreleased=0
                after_unreleased="${line}"$'\n'
            else
                unreleased_content="${unreleased_content}${line}"$'\n'
            fi
        else
            after_unreleased="${after_unreleased}${line}"$'\n'
        fi
    done < "$changelog"

    if [ "$found_unreleased" -eq 0 ]; then
        ralph_log_error "Could not find ## [Unreleased] section in CHANGELOG.md"
        rm -f "$temp_file"
        return 1
    fi

    unreleased_content=$(echo "$unreleased_content" | sed -e '/./,$!d' -e :a -e '/^\n*$/{$d;N;};/\n$/ba')

    {
        echo -n "$before_unreleased"
        echo "## [Unreleased]"
        echo ""
        echo "## [$version] - $today"
        echo ""
        if [ -n "$unreleased_content" ]; then
            echo "$unreleased_content"
            echo ""
        fi
        echo -n "$after_unreleased"
    } > "$temp_file"

    if grep -q '^\[Unreleased\]:' "$temp_file"; then
        sed -i.bak \
            -e "/^\[$version\]: /d" \
            -e "s|^\[Unreleased\]: .*|[Unreleased]: $REPO_HTTP_URL/compare/v$version...HEAD|" \
            -e "/^\[Unreleased\]: /a\\
[$version]: $REPO_HTTP_URL/compare/v$unreleased_base_version...v$version" \
            "$temp_file"
        rm -f "$temp_file.bak"
    else
        {
            echo ""
            echo "[Unreleased]: $REPO_HTTP_URL/compare/v$version...HEAD"
            echo "[$version]: $REPO_HTTP_URL/compare/v$unreleased_base_version...v$version"
        } >> "$temp_file"
    fi

    mv "$temp_file" "$changelog"
}
