#!/usr/bin/env python3
"""
Purpose: Enforce Ralph file-size policy for human-authored repository files.
Responsibilities:
- Discover tracked and untracked non-ignored files when git metadata is present.
- Fall back to deterministic repository walking when git metadata is unavailable.
- Apply explicit include/exclude policy with configurable extra exclude globs.
- Report soft-limit and hard-limit offenders with actionable path/line details.
Scope:
- Read-only policy verification only; this script never rewrites repository files.
Usage:
- python3 scripts/lib/file_size_limits.py /path/to/repo
- python3 scripts/lib/file_size_limits.py /path/to/repo --exclude-glob 'docs/generated/**'
- python3 scripts/lib/file_size_limits.py /path/to/repo --soft-limit 850 --hard-limit 1100
Invariants/assumptions:
- AGENTS.md remains the canonical threshold policy unless maintainers supersede it.
- Hard-limit violations are blocking; soft-limit violations are reported non-blocking.
"""

from __future__ import annotations

import argparse
import fnmatch
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

SOFT_LIMIT_DEFAULT = 800
HARD_LIMIT_DEFAULT = 1000

INCLUDE_SUFFIXES = {
    ".md",
    ".py",
    ".rs",
    ".sh",
    ".swift",
    ".jsonc",
}
INCLUDE_BASENAMES = {"AGENTS.md", "Makefile"}
DEFAULT_EXCLUDE_GLOBS = (
    ".git/**",
    "target/**",
    ".ralph/done.jsonc",
    ".ralph/queue.jsonc",
    ".ralph/config.jsonc",
    ".ralph/cache/**",
    ".ralph/workspaces/**",
    ".ralph/lock/**",
    ".ralph/logs/**",
    ".venv/**",
    ".pytest_cache/**",
    ".ty_cache/**",
    "docs/assets/images/**",
    "schemas/*.json",
    "apps/**/*.xcodeproj/project.pbxproj",
)
EXTRA_EXCLUDE_ENV_VAR = "RALPH_FILE_SIZE_EXCLUDE_GLOBS"


@dataclass(frozen=True)
class Offender:
    rel_path: str
    line_count: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Enforce Ralph file-size limits for human-authored repository files.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Exit codes:\n"
            "  0  no hard-limit violations (soft-limit warnings may still be reported)\n"
            "  1  one or more hard-limit violations found\n"
            "  2  usage or argument error"
        ),
    )
    parser.add_argument(
        "repo_root",
        nargs="?",
        default=".",
        help="Repository root to scan (default: current working directory)",
    )
    parser.add_argument(
        "--soft-limit",
        type=int,
        default=SOFT_LIMIT_DEFAULT,
        help=f"Soft line-count threshold (default: {SOFT_LIMIT_DEFAULT})",
    )
    parser.add_argument(
        "--hard-limit",
        type=int,
        default=HARD_LIMIT_DEFAULT,
        help=f"Hard line-count threshold (default: {HARD_LIMIT_DEFAULT})",
    )
    parser.add_argument(
        "--exclude-glob",
        action="append",
        default=[],
        help=(
            "Additional fnmatch glob to exclude (repeatable). "
            f"You can also set {EXTRA_EXCLUDE_ENV_VAR} as newline-separated globs."
        ),
    )
    return parser.parse_args()


def parse_env_patterns(raw: str) -> list[str]:
    patterns: list[str] = []
    for line in raw.splitlines():
        pattern = line.strip()
        if pattern:
            patterns.append(pattern)
    return patterns


def unique_ordered(values: Iterable[str]) -> list[str]:
    seen: set[str] = set()
    ordered: list[str] = []
    for value in values:
        if value in seen:
            continue
        seen.add(value)
        ordered.append(value)
    return ordered


def normalize_rel_path(raw_path: str) -> str:
    normalized = raw_path.replace("\\", "/")
    while normalized.startswith("./"):
        normalized = normalized[2:]
    return normalized


def is_git_worktree(repo_root: Path) -> bool:
    result = subprocess.run(
        ["git", "-C", str(repo_root), "rev-parse", "--is-inside-work-tree"],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.returncode == 0 and result.stdout.strip() == "true"


def git_list_files(repo_root: Path, args: Sequence[str]) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(repo_root), *args],
        check=False,
        capture_output=True,
    )
    if result.returncode != 0:
        stderr = result.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"git {' '.join(args)} failed: {stderr}")

    paths: list[str] = []
    for raw in result.stdout.split(b"\0"):
        if not raw:
            continue
        decoded = raw.decode("utf-8", errors="surrogateescape")
        paths.append(normalize_rel_path(decoded))
    return paths


def walk_repo_files(repo_root: Path) -> list[str]:
    rel_paths: list[str] = []
    for current_root, dirnames, filenames in os.walk(repo_root):
        current_root_path = Path(current_root)
        rel_dir = current_root_path.relative_to(repo_root).as_posix()

        if rel_dir == ".":
            rel_dir = ""

        dirnames[:] = [
            name
            for name in dirnames
            if name
            not in {
                ".git",
                "target",
                ".venv",
                ".pytest_cache",
                ".ty_cache",
            }
        ]

        for filename in filenames:
            rel_path = "/".join(part for part in (rel_dir, filename) if part)
            rel_paths.append(rel_path)
    return rel_paths


def discover_candidate_paths(repo_root: Path) -> list[str]:
    if is_git_worktree(repo_root):
        tracked = git_list_files(repo_root, ["ls-files", "-z"])
        untracked = git_list_files(
            repo_root,
            ["ls-files", "--others", "--exclude-standard", "-z"],
        )
        return sorted(set(tracked) | set(untracked))

    return sorted(set(walk_repo_files(repo_root)))


def is_excluded(rel_path: str, exclude_patterns: Sequence[str]) -> bool:
    return any(fnmatch.fnmatchcase(rel_path, pattern) for pattern in exclude_patterns)


def should_check(rel_path: str, exclude_patterns: Sequence[str]) -> bool:
    if is_excluded(rel_path, exclude_patterns):
        return False

    filename = Path(rel_path).name
    if filename in INCLUDE_BASENAMES:
        return True

    return Path(rel_path).suffix in INCLUDE_SUFFIXES


def count_lines(path: Path) -> int:
    data = path.read_bytes()
    if not data:
        return 0
    return data.count(b"\n") + (0 if data.endswith(b"\n") else 1)


def classify_offenders(
    repo_root: Path,
    rel_paths: Sequence[str],
    soft_limit: int,
    hard_limit: int,
    exclude_patterns: Sequence[str],
) -> tuple[list[Offender], list[Offender], int]:
    soft_offenders: list[Offender] = []
    hard_offenders: list[Offender] = []
    checked_files = 0

    for rel_path in rel_paths:
        rel_path = normalize_rel_path(rel_path)
        if not should_check(rel_path, exclude_patterns):
            continue

        abs_path = repo_root / rel_path
        if not abs_path.exists() or not abs_path.is_file():
            continue

        try:
            line_count = count_lines(abs_path)
        except OSError:
            continue

        checked_files += 1
        if line_count > hard_limit:
            hard_offenders.append(Offender(rel_path=rel_path, line_count=line_count))
        elif line_count > soft_limit:
            soft_offenders.append(Offender(rel_path=rel_path, line_count=line_count))

    soft_offenders.sort(key=lambda item: (-item.line_count, item.rel_path))
    hard_offenders.sort(key=lambda item: (-item.line_count, item.rel_path))
    return soft_offenders, hard_offenders, checked_files


def print_offenders(label: str, offenders: Sequence[Offender]) -> None:
    print(label)
    for offender in offenders:
        print(f"  {offender.line_count:>5}  {offender.rel_path}")


def main() -> int:
    args = parse_args()

    if args.soft_limit <= 0 or args.hard_limit <= 0:
        print("ERROR: --soft-limit and --hard-limit must be positive integers", file=sys.stderr)
        return 2
    if args.soft_limit >= args.hard_limit:
        print("ERROR: --soft-limit must be less than --hard-limit", file=sys.stderr)
        return 2

    repo_root = Path(args.repo_root).resolve()
    if not repo_root.exists() or not repo_root.is_dir():
        print(f"ERROR: repo root is not a directory: {repo_root}", file=sys.stderr)
        return 2

    env_patterns = parse_env_patterns(os.environ.get(EXTRA_EXCLUDE_ENV_VAR, ""))
    exclude_patterns = unique_ordered(
        [*DEFAULT_EXCLUDE_GLOBS, *env_patterns, *args.exclude_glob]
    )

    try:
        rel_paths = discover_candidate_paths(repo_root)
    except RuntimeError as err:
        print(f"ERROR: {err}", file=sys.stderr)
        return 2

    soft_offenders, hard_offenders, checked_files = classify_offenders(
        repo_root=repo_root,
        rel_paths=rel_paths,
        soft_limit=args.soft_limit,
        hard_limit=args.hard_limit,
        exclude_patterns=exclude_patterns,
    )

    print(
        "Checked "
        f"{checked_files} files (soft>{args.soft_limit}, hard>{args.hard_limit})"
    )

    if soft_offenders:
        print_offenders("WARN: soft file-size limit exceeded:", soft_offenders)
        print("WARN: soft-limit offenders are non-blocking but should be split.")

    if hard_offenders:
        print_offenders("ERROR: hard file-size limit exceeded:", hard_offenders)
        return 1

    if not soft_offenders:
        print("OK: file-size limits within policy")

    return 0


if __name__ == "__main__":
    sys.exit(main())
