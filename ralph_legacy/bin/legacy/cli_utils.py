"""Common CLI argument helpers for consistent tooling interfaces.

Entrypoints: cli_error, cli_examples, docstring_epilog, and the add_* helpers.
"""

from __future__ import annotations

import argparse
import os
from typing import NoReturn

from rich.console import Console

DOCSTRING_SECTION_ORDER = [
    "PREREQUISITES",
    "DATA SOURCE",
    "DATA SOURCES",
    "DATA COLLECTED",
    "TARGET AREAS",
    "SEARCH OPTIMIZATION",
    "MODE",
    "USAGE",
    "SUBCOMMANDS",
    "SCAN PROFILES",
    "SCAN COLUMNS",
    "MATCHING METHODS",
    "MATCHING STRATEGIES",
    "MATCHING ALGORITHM",
    "NETWORK DETECTION",
    "DETECTION TARGETS",
    "RISK SCORING",
    "VENDOR SCORING NOTES",
    "RED FLAGS",
    "RED FLAGS SCORED",
    "PACKET CONTENTS",
    "REPORT CONTENTS",
    "EXTRACTED FIELDS",
    "OUTPUT FORMAT",
    "OUTPUT",
    "NOTES",
    "NOTE",
    "API LIMITATIONS",
    "OVERRIDES FORMAT",
    "STATUS VALUES",
    "MIGRATION",
    "DEFAULT EXCLUSIONS",
    "ORPHANED - NOT IN ACTIVE BUILD CHAIN",
    "EXAMPLES",
]

DB_ENV_HELP = (
    "Database is configured via env: IDF_DB_DSN "
    "(legacy: IDCF_DB_DSN; fallback: IDF_VIEWER_DB_DSN/IDCF_VIEWER_DB_DSN or POSTGRES_*)."
)


def cli_error(
    message: str, *, code: int = 1, console: Console | None = None
) -> NoReturn:
    """Print a standardized CLI error message and exit."""
    target = console or Console(stderr=True)
    target.print(f"[bold red]Error:[/bold red] {message}")
    raise SystemExit(code)


def cli_examples(*lines: str) -> str:
    """Format an EXAMPLES block for argparse epilog usage."""
    cleaned: list[str] = []
    for line in lines:
        if not line:
            cleaned.append("")
            continue
        cleaned.append(line if line.startswith("  ") else f"  {line}")
    return "EXAMPLES\n--------\n" + "\n".join(cleaned)


def docstring_epilog(doc: str | None) -> str | None:
    """Return a docstring for argparse epilog use."""
    return doc


def add_db_env_hint(parser: argparse.ArgumentParser) -> None:
    """Append a database configuration hint to parser descriptions."""
    description = parser.description or ""
    suffix = f"\n\n{DB_ENV_HELP}"
    parser.description = f"{description}{suffix}" if description else DB_ENV_HELP


def add_run_id_arg(
    parser: argparse.ArgumentParser,
    *,
    required: bool = False,
    help_text: str | None = None,
    default: str | None = None,
) -> None:
    resolved_default = default
    if resolved_default is None and not required:
        resolved_default = ""
    parser.add_argument(
        "--run-id",
        required=required,
        default=resolved_default,
        help=help_text or "Build run ID (required for reproducibility)"
        if required
        else "Build run ID",
    )


def add_fiscal_year_args(
    parser: argparse.ArgumentParser,
    *,
    default_from: int = 2020,
    default_to: int | None = None,
) -> None:
    parser.add_argument(
        "--from-fiscal-year",
        type=int,
        default=default_from,
        help=f"Start fiscal year (default: {default_from})",
    )
    parser.add_argument(
        "--to-fiscal-year",
        type=int,
        default=default_to,
        help="End fiscal year (default: current year)",
    )


def add_account_type_arg(
    parser: argparse.ArgumentParser,
    *,
    default: str = "expense",
    help_text: str | None = None,
) -> None:
    parser.add_argument(
        "--account-type",
        default=default,
        choices=["expense", "all"],
        help=help_text or "OpenGov account type filter (default: expense).",
    )


def add_limit_arg(
    parser: argparse.ArgumentParser,
    *,
    default: int | None = None,
    help_text: str | None = None,
) -> None:
    parser.add_argument(
        "--limit",
        type=int,
        default=default,
        help=help_text or "Optional limit for debugging",
    )


def add_workers_arg(
    parser: argparse.ArgumentParser,
    *,
    default: int,
    help_text: str | None = None,
) -> None:
    parser.add_argument(
        "--workers",
        "--max-workers",
        dest="workers",
        type=int,
        default=default,
        help=(help_text or "Parallel workers (default: auto)")
        + " (alias: --max-workers deprecated)",
    )


def resolve_worker_count(
    value: int | None,
    *,
    default_cap: int = 24,
    env_var: str = "IDF_JOB_WORKERS_MAX",
    legacy_env_var: str = "IDCF_JOB_WORKERS_MAX",
) -> int:
    """Resolve worker count with a per-job cap.

    Treats 0/None as auto = CPU count. Caps both auto and explicit values using
    IDF_JOB_WORKERS_MAX (legacy: IDCF_JOB_WORKERS_MAX) or the provided default_cap.
    """
    requested = value if value and value > 0 else (os.cpu_count() or 1)
    cap_raw = os.getenv(env_var)
    if not cap_raw:
        cap_raw = os.getenv(legacy_env_var)
    if cap_raw and cap_raw.strip():
        try:
            cap_value = int(cap_raw)
        except ValueError as exc:
            raise ValueError(
                f"{env_var} (legacy: {legacy_env_var}) must be an integer"
            ) from exc
    else:
        cap_value = default_cap
    if cap_value <= 0:
        raise ValueError(f"{env_var} (legacy: {legacy_env_var}) must be >= 1")
    return max(1, min(requested, cap_value))


def add_apply_dry_run_args(
    parser: argparse.ArgumentParser,
    *,
    default_apply: bool = False,
    apply_help: str | None = None,
    dry_run_help: str | None = None,
) -> None:
    """Add mutually exclusive --apply and --dry-run arguments.

    Uses a single destination (dest="apply") so there is only one source of truth.
    Default behavior is controlled by default_apply.

    Args:
        parser: The argument parser to add arguments to.
        default_apply: Default value for apply (False = dry-run default).
        apply_help: Custom help text for --apply.
        dry_run_help: Custom help text for --dry-run.
    """
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--apply",
        dest="apply",
        action="store_true",
        help=apply_help or "Apply changes (default: dry-run)",
    )
    mode.add_argument(
        "--dry-run",
        dest="apply",
        action="store_false",
        help=dry_run_help or "Dry-run without applying changes (default)",
    )
    parser.set_defaults(apply=default_apply)
