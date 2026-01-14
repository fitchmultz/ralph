#!/usr/bin/env python3
"""Deterministic helpers for moving Ralph queue items between sections.

Entrypoint: main.

EXAMPLES
--------
  ralph_legacy/bin/legacy/pin_ops.py move-checked \
    --queue ralph_legacy/specs/implementation_queue.md \
    --done ralph_legacy/specs/implementation_done.md \
    --prepend

  ralph_legacy/bin/legacy/pin_ops.py block-item \
    --queue ralph_legacy/specs/implementation_queue.md \
    --item-id IDFQ-0123 \
    --reason "make ci failed after 2 supervisor attempts" \
    --reason "Unblock: fix the failing check and requeue" \
    --wip-branch ralph/wip/IDFQ-0123/20260112_031122 \
    --known-good 1234abcd
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

from cli_utils import cli_error, cli_examples, docstring_epilog
from rich.console import Console

ID_PATTERN = re.compile(r"[A-Z0-9]{2,10}-\d{4}")
BLOCK_START_PREFIXES = ("- [", "## ")


def split_blocks(lines: list[str]) -> list[list[str]]:
    """Split lines into blocks anchored by task or section headers."""
    blocks: list[list[str]] = []
    current: list[str] = []

    for line in lines:
        if line.startswith(BLOCK_START_PREFIXES):
            if current:
                blocks.append(current)
            current = [line]
            continue
        if current:
            current.append(line)
        else:
            blocks.append([line])
            current = []

    if current:
        blocks.append(current)
    return blocks


def summarize_ids(ids: list[str]) -> str:
    """Return a human-friendly summary for a list of IDs."""
    if not ids:
        return ""
    unique_ids = list(dict.fromkeys(ids))
    if len(unique_ids) == 1:
        return unique_ids[0]
    if len(unique_ids) == 2:
        return f"{unique_ids[0]}, {unique_ids[1]}"
    return f"{unique_ids[0]} +{len(unique_ids) - 1}"


def ensure_done_section(done_lines: list[str]) -> int:
    """Ensure the Done section header exists and return its index."""
    if "## Done" not in done_lines:
        insert_at = 1 if done_lines and done_lines[0].startswith("#") else 0
        done_lines.insert(insert_at, "## Done")
    return done_lines.index("## Done")


def find_section_end(blocks: list[list[str]], header_index: int) -> int:
    """Return the block index where a section ends (next header or end)."""
    idx = header_index + 1
    while idx < len(blocks):
        header = blocks[idx][0] if blocks[idx] else ""
        if header.startswith("## "):
            break
        idx += 1
    return idx


def move_checked_to_done(
    queue_path: Path,
    done_path: Path,
    *,
    prepend: bool,
) -> list[str]:
    """Move - [x] blocks from ## Queue into Done log, return IDs moved."""
    if not queue_path.exists():
        cli_error(f"Queue file not found: {queue_path}")
    if not done_path.exists():
        cli_error(f"Done log not found: {done_path}")

    queue_lines = queue_path.read_text().splitlines()
    done_lines = done_path.read_text().splitlines()

    blocks = split_blocks(queue_lines)
    new_queue: list[str] = []
    moved: list[list[str]] = []
    in_queue = False
    ids: list[str] = []

    for block in blocks:
        header = block[0] if block else ""
        if header.strip() == "## Queue":
            in_queue = True
            new_queue.extend(block)
            continue
        if header.startswith("## "):
            in_queue = False
            new_queue.extend(block)
            continue
        if in_queue and header.startswith("- [x]"):
            moved.append(block)
            match = ID_PATTERN.search(header)
            if match:
                ids.append(match.group(0))
            continue
        new_queue.extend(block)

    if moved:
        done_idx = ensure_done_section(done_lines)
        insert_pos = done_idx + 1

        inserted: list[str] = []
        for block in moved:
            inserted.extend(block)

        if prepend:
            done_lines[insert_pos:insert_pos] = inserted
        else:
            section_end = len(done_lines)
            for idx in range(done_idx + 1, len(done_lines)):
                if done_lines[idx].startswith("## "):
                    section_end = idx
                    break
            done_lines[section_end:section_end] = inserted

        done_path.write_text("\n".join(done_lines) + "\n")

    queue_path.write_text("\n".join(new_queue) + "\n")
    return list(dict.fromkeys(ids))


def append_metadata(
    block: list[str], reason_lines: list[str], metadata: dict[str, str]
) -> None:
    """Append standardized metadata bullets to a queue item block."""
    indent = "  "
    for line in reason_lines:
        cleaned = line.strip()
        if cleaned:
            block.append(f"{indent}- Blocked reason: {cleaned}")
    for label, key in (
        ("WIP branch", "wip_branch"),
        ("Known-good", "known_good"),
        ("Unblock hint", "unblock_hint"),
    ):
        value = metadata.get(key)
        if value:
            block.append(f"{indent}- {label}: {value}")


def block_item(
    queue_path: Path,
    *,
    item_id: str,
    reason_lines: list[str],
    metadata: dict[str, str],
) -> bool:
    """Move the item block from ## Queue to ## Blocked and append metadata bullets."""
    if not queue_path.exists():
        cli_error(f"Queue file not found: {queue_path}")

    lines = queue_path.read_text().splitlines()
    blocks = split_blocks(lines)
    new_blocks: list[list[str]] = []

    in_queue = False
    queue_index: int | None = None
    blocked_index: int | None = None
    item_block: list[str] | None = None

    for block in blocks:
        header = block[0] if block else ""
        if header.strip() == "## Queue":
            in_queue = True
            queue_index = len(new_blocks)
            new_blocks.append(block)
            continue
        if header.strip() == "## Blocked":
            in_queue = False
            blocked_index = len(new_blocks)
            new_blocks.append(block)
            continue
        if header.startswith("## "):
            in_queue = False
            new_blocks.append(block)
            continue
        if in_queue and item_id in header and header.startswith("- ["):
            item_block = block
            continue
        new_blocks.append(block)

    if item_block is None:
        return False

    append_metadata(item_block, reason_lines, metadata)

    if blocked_index is not None:
        insert_pos = blocked_index + 1
        new_blocks.insert(insert_pos, item_block)
    else:
        if queue_index is None:
            cli_error("Queue section not found while blocking item.")
        insert_pos = find_section_end(new_blocks, queue_index)
        new_blocks.insert(insert_pos, ["## Blocked"])
        new_blocks.insert(insert_pos + 1, item_block)

    flattened: list[str] = []
    for block in new_blocks:
        flattened.extend(block)

    queue_path.write_text("\n".join(flattened) + "\n")
    return True


def build_parser() -> argparse.ArgumentParser:
    """Build the CLI parser."""
    parser = argparse.ArgumentParser(
        description="Move Ralph queue items between sections with deterministic rules.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=docstring_epilog(__doc__),
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    move_parser = subparsers.add_parser(
        "move-checked",
        help="Move checked queue items into the done log.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=cli_examples(
            "ralph_legacy/bin/legacy/pin_ops.py move-checked --queue ralph_legacy/specs/implementation_queue.md \\",
            "  --done ralph_legacy/specs/implementation_done.md --prepend",
        ),
    )
    move_parser.add_argument(
        "--queue", type=Path, required=True, help="Queue markdown file"
    )
    move_parser.add_argument(
        "--done", type=Path, required=True, help="Done log markdown file"
    )
    move_parser.add_argument(
        "--prepend",
        action="store_true",
        help="Insert moved items at the top of the Done section",
    )

    block_parser = subparsers.add_parser(
        "block-item",
        help="Move a queue item into Blocked with metadata.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=cli_examples(
            "ralph_legacy/bin/legacy/pin_ops.py block-item --queue ralph_legacy/specs/implementation_queue.md \\",
            '  --item-id IDFQ-0123 --reason "make ci failed after 2 attempts" \\',
            "  --wip-branch ralph/wip/IDFQ-0123/20260112_031122 --known-good 1234abcd",
        ),
    )
    block_parser.add_argument(
        "--queue", type=Path, required=True, help="Queue markdown file"
    )
    block_parser.add_argument(
        "--item-id", required=True, help="Queue item ID (e.g., IDFQ-0123)"
    )
    block_parser.add_argument(
        "--reason",
        action="append",
        default=[],
        help="Reason line (repeatable)",
    )
    block_parser.add_argument(
        "--wip-branch", help="WIP branch name containing quarantined work"
    )
    block_parser.add_argument(
        "--known-good", help="Known-good Git SHA before the failure"
    )
    block_parser.add_argument("--unblock-hint", help="Hint describing how to unblock")

    return parser


def main() -> int:
    """Run the CLI."""
    parser = build_parser()
    args = parser.parse_args()

    if args.command == "move-checked":
        moved_ids = move_checked_to_done(
            args.queue,
            args.done,
            prepend=args.prepend,
        )
        summary = summarize_ids(moved_ids)
        if summary:
            Console().print(summary, markup=False)
        return 0

    if args.command == "block-item":
        reason_lines: list[str] = []
        for reason in args.reason:
            reason_lines.extend([line for line in reason.splitlines() if line.strip()])
        metadata = {
            "wip_branch": args.wip_branch or "",
            "known_good": args.known_good or "",
            "unblock_hint": args.unblock_hint or "",
        }
        if not reason_lines:
            cli_error("At least one --reason line is required to block an item.")
        if not block_item(
            args.queue,
            item_id=args.item_id,
            reason_lines=reason_lines,
            metadata=metadata,
        ):
            cli_error(f"Item {args.item_id} not found in Queue.")
        return 0

    cli_error(f"Unknown command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
