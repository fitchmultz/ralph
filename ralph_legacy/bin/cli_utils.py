"""Common CLI helpers for legacy Ralph scripts.

Entrypoints: cli_error, cli_examples, docstring_epilog.
"""

from __future__ import annotations

from typing import NoReturn

from rich.console import Console


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
