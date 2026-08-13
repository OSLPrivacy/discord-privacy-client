#!/usr/bin/env python3
"""Fail-closed verifier for TASK 4089's live Signal row-words receipts."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


class CheckError(RuntimeError):
    pass


def load(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CheckError(f"could not read receipt {path}: {error}") from error


def check_common(receipt: dict) -> None:
    if receipt.get("schema") != "task-4089-signal-live-row-words/v1":
        raise CheckError("bad TASK4089 schema")
    if receipt.get("task") != 4089 or receipt.get("source") != "live Windows UI Automation":
        raise CheckError("not a live TASK4089 Signal receipt")
    if receipt.get("winner") != "row.name":
        raise CheckError(f"winner={receipt.get('winner')!r} is not the TASK4088 winning route")
    if receipt.get("saved_ui_trees") != 0:
        raise CheckError("saved UI trees must be 0")
    if receipt.get("key_press_count") != 0:
        raise CheckError("key press count must be 0")
    if receipt.get("send_count") != 0:
        raise CheckError("send count must be 0")
    capture = receipt.get("capture", {})
    if capture.get("method") != "Windows.PowerShell.CopyFromScreen":
        raise CheckError("CopyFromScreen capture missing")
    if not isinstance(capture.get("distinct_colours"), int) or capture["distinct_colours"] <= 16:
        raise CheckError(f"CopyFromScreen distinct_colours={capture.get('distinct_colours')}")
    if capture.get("brightness_judgments") != "forbidden-and-not-used":
        raise CheckError("brightness judgment was used or not forbidden")


def check_before(receipt: dict) -> str:
    check_common(receipt)
    matched = receipt.get("matched_count")
    if matched != 0:
        raise CheckError(f"before receipt matched_count={matched}, expected 0")
    return f"TASK4089 before green matched_count=0 row_count_seen={receipt.get('row_count_seen')}"


def check_after(receipt: dict, markers: list[str]) -> str:
    check_common(receipt)
    messages = receipt.get("messages")
    if not isinstance(messages, list):
        raise CheckError("after receipt has no messages list")
    texts = [message.get("text") for message in messages]
    empty = [text for text in texts if not text]
    if empty:
        raise CheckError(f"{len(empty)} empty texts among returned rows")
    starved = [marker for marker in markers if not any(marker in text for text in texts)]
    if starved:
        raise CheckError(
            f"starved markers={','.join(starved)}"
        )
    if len(texts) != len(markers):
        raise CheckError(f"after receipt returned {len(texts)} rows, expected {len(markers)}")
    if len(set(texts)) != len(texts):
        raise CheckError("after receipt returned duplicate row texts")
    return (
        f"TASK4089 after green rows={len(texts)} empty=0 "
        f"matched_count={receipt.get('matched_count')} colours={receipt['capture']['distinct_colours']}"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--before", action="store_true")
    parser.add_argument("--after", action="store_true")
    parser.add_argument("--marker", action="append", default=[], help="expected unique marker token; repeatable")
    args = parser.parse_args()
    try:
        receipt = load(args.receipt)
        if args.before and args.after:
            raise CheckError("choose exactly one of --before / --after")
        if args.before:
            print(check_before(receipt))
        elif args.after:
            if not args.marker:
                raise CheckError("--after requires at least one --marker")
            print(check_after(receipt, args.marker))
        else:
            raise CheckError("choose --before or --after")
        return 0
    except CheckError as error:
        print(f"TASK4089 exit 1: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
