#!/usr/bin/env python3
"""
Purpose: Scan the Ralph working tree for public-readiness issues.
Responsibilities:
- Walk the repository working tree with explicit exclude rules.
- Validate markdown links across repo-local documentation files.
- Reject stale documented session-cache paths that omit the JSONC extension.
- Reject markdown links that resolve outside the repository root.
- Detect high-confidence secret patterns in working-tree text files outside local-only exclusions.
Scope:
- Repository working-tree scanning only; release orchestration stays in shell scripts.
Usage:
- python3 scripts/lib/public_readiness_scan.py links /path/to/repo
- python3 scripts/lib/public_readiness_scan.py session-paths /path/to/repo
- python3 scripts/lib/public_readiness_scan.py secrets /path/to/repo
- python3 scripts/lib/public_readiness_scan.py docs /path/to/repo
- python3 scripts/lib/public_readiness_scan.py all /path/to/repo
Invariants/assumptions:
- The caller provides the repository root as the final argument.
- Markdown link targets must resolve within the repository root.
- Excludes are provided through RALPH_PUBLIC_SCAN_EXCLUDES as newline-separated prefixes.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Iterator


MARKDOWN_LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
MARKDOWN_JSON_FENCE_RE = re.compile(r"```json\s*\n(?P<body>[\s\S]*?)\n```", re.MULTILINE)
STALE_SESSION_CACHE_PATH_RE = re.compile(r"\.ralph/cache/session\.json(?!c)")
AWS_EXAMPLE_KEY = "AK" "IA" "IOSFODNN7EXAMPLE"
OPENSSH_PRIVATE_KEY_TAG = "OPEN" "SSH PRIVATE KEY"
RSA_PRIVATE_KEY_TAG = "RSA PRIVATE KEY"
OPENSSH_PRIVATE_KEY_HEADER = "BEGIN " + OPENSSH_PRIVATE_KEY_TAG
RSA_PRIVATE_KEY_HEADER = "BEGIN " + RSA_PRIVATE_KEY_TAG
OPENSSH_PRIVATE_KEY_LINE = f"-----{OPENSSH_PRIVATE_KEY_HEADER}-----"
RSA_PRIVATE_KEY_LINE = f"-----{RSA_PRIVATE_KEY_HEADER}-----"
OPENSSH_PRIVATE_KEY_FOOTER = f"-----END {OPENSSH_PRIVATE_KEY_TAG}-----"
RSA_PRIVATE_KEY_FOOTER = f"-----END {RSA_PRIVATE_KEY_TAG}-----"
AWS_DOCS_ALLOWLIST_LINE = (
    f"| **AWS Keys** | AKIA-prefixed access keys | `{AWS_EXAMPLE_KEY}` → `[REDACTED]` |"
)
REDACTION_EXPANSION_ALLOWLIST_LINES = {
    f'"My key is {AWS_EXAMPLE_KEY} and secret is wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";',
    f'assert!(!output.contains("{AWS_EXAMPLE_KEY}"));',
    OPENSSH_PRIVATE_KEY_LINE,
    (
        f'let input = "private_key: |\\n  {RSA_PRIVATE_KEY_LINE}\\n'
        '  MIIEpAIBAAKCAQEA75...\\n'
        f'  {RSA_PRIVATE_KEY_FOOTER}";'
    ),
}
FSUTIL_ALLOWLIST_LINES = {
    f'let content = "AWS Access Key: {AWS_EXAMPLE_KEY}";',
    f'!written.contains("{AWS_EXAMPLE_KEY}"),',
    f'"SSH Key:\\n{OPENSSH_PRIVATE_KEY_LINE}\\nabc123\\n{OPENSSH_PRIVATE_KEY_FOOTER}";',
}
HIGH_CONFIDENCE_SECRET_PATTERNS = {
    "aws_access_key": re.compile(r"AK" r"IA[0-9A-Z]{16}"),
    "github_classic_token": re.compile(r"gh[pousr]" r"_[A-Za-z0-9]{20,}"),
    "github_pat": re.compile(r"github_pat" r"_[A-Za-z0-9_]{20,}"),
    "slack_token": re.compile(r"xox[baprs]" r"-[A-Za-z0-9-]{10,}"),
    "openai_key": re.compile(r"sk" r"-(?:proj-)?[A-Za-z0-9]{20,}"),
    "anthropic_key": re.compile(r"sk-ant" r"-[A-Za-z0-9_-]{20,}"),
    "npm_token": re.compile(r"npm" r"_[A-Za-z0-9]{36}"),
    "stripe_live": re.compile(r"sk_live" r"_[A-Za-z0-9]{16,}"),
    "private_key": re.compile(
        r"BEGIN (?:RSA|OPEN" r"SSH|EC|DSA|PGP) PRIVATE " r"KEY"
    ),
}


@dataclass(frozen=True)
class DocSnippetContract:
    rel_path: str
    pattern: re.Pattern[str]
    message: str
    search_scope: str = "document"


STALE_DOC_SNIPPET_CONTRACTS = (
    DocSnippetContract(
        rel_path="docs/features/app.md",
        pattern=re.compile(r"`ralph task decompose --format json`"),
        message="use `ralph machine task decompose` for RalphMac decomposition docs",
    ),
    DocSnippetContract(
        rel_path="docs/features/session-management.md",
        pattern=re.compile(
            r'"version"\s*:\s*3[\s\S]*?"resume_preview"',
            re.MULTILINE,
        ),
        message="machine config resolve examples must use version 4",
        search_scope="json_blocks",
    ),
    DocSnippetContract(
        rel_path="docs/features/session-management.md",
        pattern=re.compile(
            r'"version"\s*:\s*2[\s\S]*?"kind"\s*:\s*"resume_decision"',
            re.MULTILINE,
        ),
        message="machine run resume_decision examples must use version 3",
        search_scope="json_blocks",
    ),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("mode", choices=("links", "secrets", "session-paths", "docs", "all"))
    parser.add_argument("repo_root")
    return parser.parse_args()


@lru_cache(maxsize=None)
def read_env_lines(name: str) -> tuple[str, ...]:
    raw = os.environ.get(name, "")
    values = []
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        values.append(line)
    return tuple(values)


@lru_cache(maxsize=1)
def read_excludes() -> tuple[str, ...]:
    return tuple(line.rstrip("/") for line in read_env_lines("RALPH_PUBLIC_SCAN_EXCLUDES"))


@lru_cache(maxsize=1)
def read_local_only_basenames() -> tuple[str, ...]:
    configured = read_env_lines("RALPH_PUBLIC_SCAN_LOCAL_ONLY_BASENAMES")
    if configured:
        return configured
    return (".DS_Store", ".env", ".envrc", ".scratchpad.md", ".FIX_TRACKING.md")


@lru_cache(maxsize=1)
def read_local_only_basename_prefixes() -> tuple[str, ...]:
    configured = read_env_lines("RALPH_PUBLIC_SCAN_LOCAL_ONLY_BASENAME_PREFIXES")
    if configured:
        return configured
    return (".env.",)


def is_local_only_name(name: str) -> bool:
    if name in read_local_only_basenames():
        return True
    for prefix in read_local_only_basename_prefixes():
        if name.startswith(prefix) and name != ".env.example":
            return True
    return False


def is_locally_sensitive_file(rel_path: str) -> bool:
    return any(is_local_only_name(part) for part in Path(rel_path).parts if part not in ("", "."))


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


def read_text(path: Path, repo_root: Path, excludes: tuple[str, ...]) -> str | None:
    # Skip symlinks that escape the working tree or land in excluded paths.
    # Repo-local symlinks that resolve to scan-visible files are still scanned.
    if path.is_symlink():
        try:
            resolved = path.resolve()
            rel_resolved = resolved.relative_to(repo_root).as_posix()
        except (OSError, ValueError):
            return None
        if is_excluded(rel_resolved, excludes):
            return None
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
    stripped = line.strip()
    if rel_path == "crates/ralph/tests/redaction_expansion_test.rs":
        return stripped in REDACTION_EXPANSION_ALLOWLIST_LINES
    if rel_path == "docs/features/security.md":
        return stripped == AWS_DOCS_ALLOWLIST_LINE
    if rel_path == "crates/ralph/src/fsutil/tests.rs":
        return stripped in FSUTIL_ALLOWLIST_LINES
    return False


def render_secret_finding(name: str, matched_secret: str) -> str:
    return f"{name}: [REDACTED length={len(matched_secret)}]"


def scans_session_path_contract(rel_path: str, path: Path) -> bool:
    return path.suffix == ".md" or rel_path == "AGENTS.md" or rel_path.endswith("/AGENTS.md")


def collect_link_problems(path: Path, repo_root: Path, text: str) -> list[str]:
    problems: list[str] = []
    source_path = path.resolve() if path.is_symlink() else path
    rel_markdown_path = path.relative_to(repo_root).as_posix()
    for raw_target in MARKDOWN_LINK_RE.findall(text):
        target = raw_target.strip().split()[0].strip("<>")
        if target.startswith(("http://", "https://", "mailto:", "#")):
            continue
        if "{{" in target or "}}" in target:
            continue
        target = target.split("#", 1)[0].split("?", 1)[0]
        if not target:
            continue
        resolved = (source_path.parent / target).resolve()
        try:
            resolved.relative_to(repo_root)
        except ValueError:
            problems.append(f"{rel_markdown_path}: target escapes repo root -> {raw_target}")
            continue
        if not resolved.exists():
            problems.append(f"{rel_markdown_path}: missing target -> {raw_target}")
    return problems


def collect_session_path_problems(rel_path: str, text: str) -> list[str]:
    problems: list[str] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        if STALE_SESSION_CACHE_PATH_RE.search(line):
            problems.append(f"{rel_path}:{line_number}: use .ralph/cache/session.jsonc")
    return problems


def collect_secret_problems(rel_path: str, text: str) -> list[str]:
    problems: list[str] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        for name, pattern in HIGH_CONFIDENCE_SECRET_PATTERNS.items():
            match = pattern.search(line)
            if not match:
                continue
            if allowlisted_secret(rel_path, line):
                continue
            finding = render_secret_finding(name, match.group(0))
            problems.append(f"{rel_path}:{line_number}: {finding}")
    return problems


def iter_markdown_json_blocks(text: str) -> Iterator[str]:
    for match in MARKDOWN_JSON_FENCE_RE.finditer(text):
        yield match.group("body")


def collect_doc_contract_problems(rel_path: str, text: str) -> list[str]:
    problems: list[str] = []
    json_blocks = tuple(iter_markdown_json_blocks(text))
    for contract in STALE_DOC_SNIPPET_CONTRACTS:
        if rel_path != contract.rel_path:
            continue
        haystacks = (text,)
        if contract.search_scope == "json_blocks":
            haystacks = json_blocks
        if any(contract.pattern.search(haystack) for haystack in haystacks):
            problems.append(f"{rel_path}: {contract.message}")
    return problems


def scan_repo(
    repo_root: Path,
    excludes: tuple[str, ...],
    *,
    include_links: bool,
    include_session_paths: bool,
    include_doc_contracts: bool,
    include_secrets: bool,
) -> int:
    problems: list[str] = []
    for path in iter_repo_files(repo_root, excludes):
        rel_path = path.relative_to(repo_root).as_posix()
        should_scan_links = include_links and path.suffix == ".md"
        should_scan_session_paths = include_session_paths and scans_session_path_contract(
            rel_path, path
        )
        should_scan_doc_contracts = include_doc_contracts and path.suffix == ".md"
        should_scan_secrets = include_secrets
        if not (
            should_scan_links
            or should_scan_session_paths
            or should_scan_doc_contracts
            or should_scan_secrets
        ):
            continue

        text = read_text(path, repo_root, excludes)
        if text is None:
            continue

        if should_scan_links:
            problems.extend(collect_link_problems(path, repo_root, text))
        if should_scan_session_paths:
            problems.extend(collect_session_path_problems(rel_path, text))
        if should_scan_doc_contracts:
            problems.extend(collect_doc_contract_problems(rel_path, text))
        if should_scan_secrets:
            problems.extend(collect_secret_problems(rel_path, text))

    if problems:
        print("\n".join(problems))
        return 1
    return 0


def scan_links(repo_root: Path, excludes: tuple[str, ...]) -> int:
    return scan_repo(
        repo_root,
        excludes,
        include_links=True,
        include_session_paths=False,
        include_doc_contracts=False,
        include_secrets=False,
    )


def scan_session_paths(repo_root: Path, excludes: tuple[str, ...]) -> int:
    return scan_repo(
        repo_root,
        excludes,
        include_links=False,
        include_session_paths=True,
        include_doc_contracts=False,
        include_secrets=False,
    )


def scan_secrets(repo_root: Path, excludes: tuple[str, ...]) -> int:
    return scan_repo(
        repo_root,
        excludes,
        include_links=False,
        include_session_paths=False,
        include_doc_contracts=False,
        include_secrets=True,
    )


def scan_docs(repo_root: Path, excludes: tuple[str, ...]) -> int:
    return scan_repo(
        repo_root,
        excludes,
        include_links=True,
        include_session_paths=True,
        include_doc_contracts=True,
        include_secrets=False,
    )


def scan_all(repo_root: Path, excludes: tuple[str, ...]) -> int:
    return scan_repo(
        repo_root,
        excludes,
        include_links=True,
        include_session_paths=True,
        include_doc_contracts=True,
        include_secrets=True,
    )


def main() -> int:
    args = parse_args()
    if not args.repo_root.strip():
        print("repository root must not be empty", file=sys.stderr)
        return 2

    repo_root = Path(args.repo_root).resolve()
    if not repo_root.is_dir():
        print(
            f"repository root does not exist or is not a directory: {repo_root}",
            file=sys.stderr,
        )
        return 2

    excludes = read_excludes()

    if args.mode == "links":
        return scan_links(repo_root, excludes)
    if args.mode == "session-paths":
        return scan_session_paths(repo_root, excludes)
    if args.mode == "secrets":
        return scan_secrets(repo_root, excludes)
    if args.mode == "docs":
        return scan_docs(repo_root, excludes)
    if args.mode == "all":
        return scan_all(repo_root, excludes)
    return 2


if __name__ == "__main__":
    sys.exit(main())
