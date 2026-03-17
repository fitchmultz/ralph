#!/usr/bin/env python3
"""
Purpose: Scan the Ralph working tree for public-readiness issues.
Responsibilities:
- Walk the repository working tree with explicit exclude rules.
- Validate markdown links across repo-local documentation files.
- Detect high-confidence secret patterns in working-tree text files outside local-only exclusions.
Scope:
- Repository working-tree scanning only; release orchestration stays in shell scripts.
Usage:
- python3 scripts/lib/public_readiness_scan.py links /path/to/repo
- python3 scripts/lib/public_readiness_scan.py secrets /path/to/repo
Invariants/assumptions:
- The caller provides the repository root as the final argument.
- Excludes are provided through RALPH_PUBLIC_SCAN_EXCLUDES as newline-separated prefixes.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path
from typing import Iterator


MARKDOWN_LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
HIGH_CONFIDENCE_SECRET_PATTERNS = {
    "aws_access_key": re.compile(r"AKIA[0-9A-Z]{16}"),
    "github_classic_token": re.compile(r"gh[pousr]_[A-Za-z0-9]{20,}"),
    "github_pat": re.compile(r"github_pat_[A-Za-z0-9_]{20,}"),
    "slack_token": re.compile(r"xox[baprs]-[A-Za-z0-9-]{10,}"),
    "openai_key": re.compile(r"sk-(?:proj-)?[A-Za-z0-9]{20,}"),
    "anthropic_key": re.compile(r"sk-ant-[A-Za-z0-9_-]{20,}"),
    "npm_token": re.compile(r"npm_[A-Za-z0-9]{36}"),
    "stripe_live": re.compile(r"sk_live_[A-Za-z0-9]{16,}"),
    "private_key": re.compile(r"BEGIN (?:RSA|OPENSSH|EC|DSA|PGP) PRIVATE KEY"),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("mode", choices=("links", "secrets"))
    parser.add_argument("repo_root")
    return parser.parse_args()


def read_excludes() -> tuple[str, ...]:
    raw = os.environ.get("RALPH_PUBLIC_SCAN_EXCLUDES", "")
    excludes = []
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        excludes.append(line.rstrip("/"))
    return tuple(excludes)


def is_locally_sensitive_file(rel_path: str) -> bool:
    name = Path(rel_path).name
    if name == ".env":
        return True
    if name.startswith(".env.") and name != ".env.example":
        return True
    return name in {".scratchpad.md", ".FIX_TRACKING.md"}


def is_excluded(rel_path: str, excludes: tuple[str, ...]) -> bool:
    normalized = rel_path.replace("\\", "/")
    if normalized.startswith("./"):
        normalized = normalized[2:]
    if is_locally_sensitive_file(normalized):
        return True
    for prefix in excludes:
        if normalized == prefix or normalized.startswith(f"{prefix}/"):
            return True
    return False


def iter_repo_files(repo_root: Path, excludes: tuple[str, ...]) -> Iterator[Path]:
    for current_root, dirnames, filenames in os.walk(repo_root):
        current_root_path = Path(current_root)
        rel_dir = current_root_path.relative_to(repo_root).as_posix()
        if rel_dir == ".":
            rel_dir = ""

        dirnames[:] = [
            dirname
            for dirname in dirnames
            if not is_excluded("/".join(filter(None, [rel_dir, dirname])), excludes)
        ]

        for filename in filenames:
            rel_path = "/".join(filter(None, [rel_dir, filename]))
            if is_excluded(rel_path, excludes):
                continue
            yield repo_root / rel_path


def read_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def allowlisted_secret(rel_path: str, line: str) -> bool:
    if rel_path == "crates/ralph/tests/redaction_expansion_test.rs":
        return True
    if rel_path == "docs/features/security.md":
        return True
    if rel_path == "scripts/pre-public-check.sh" and "PRIVATE KEY" in line:
        return True
    if rel_path == "scripts/lib/public_readiness_scan.py":
        return True
    if rel_path == "crates/ralph/src/fsutil/tests.rs" and "AKIA" in line and "EXAMPLE" in line:
        return True
    if rel_path == "crates/ralph/src/fsutil/tests.rs" and "OPENSSH PRIVATE KEY" in line:
        return True
    return False


def scan_links(repo_root: Path, excludes: tuple[str, ...]) -> int:
    missing: list[str] = []
    for path in iter_repo_files(repo_root, excludes):
        if path.suffix != ".md":
            continue
        text = read_text(path)
        if text is None:
            continue
        for raw_target in MARKDOWN_LINK_RE.findall(text):
            target = raw_target.strip().split()[0].strip("<>")
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            if "{{" in target or "}}" in target:
                continue
            target = target.split("#", 1)[0].split("?", 1)[0]
            if not target:
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                missing.append(
                    f"{path.relative_to(repo_root).as_posix()}: missing target -> {raw_target}"
                )

    if missing:
        print("\n".join(missing))
        return 1
    return 0


def scan_secrets(repo_root: Path, excludes: tuple[str, ...]) -> int:
    problems: list[str] = []
    for path in iter_repo_files(repo_root, excludes):
        text = read_text(path)
        if text is None:
            continue
        rel_path = path.relative_to(repo_root).as_posix()
        for line_number, line in enumerate(text.splitlines(), start=1):
            for name, pattern in HIGH_CONFIDENCE_SECRET_PATTERNS.items():
                match = pattern.search(line)
                if not match:
                    continue
                if allowlisted_secret(rel_path, line):
                    continue
                problems.append(f"{rel_path}:{line_number}: {name}: {match.group(0)}")

    if problems:
        print("\n".join(problems))
        return 1
    return 0


def main() -> int:
    args = parse_args()
    repo_root = Path(args.repo_root).resolve()
    excludes = read_excludes()

    if args.mode == "links":
        return scan_links(repo_root, excludes)
    if args.mode == "secrets":
        return scan_secrets(repo_root, excludes)
    return 2


if __name__ == "__main__":
    sys.exit(main())
