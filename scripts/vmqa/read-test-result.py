#!/usr/bin/python3
"""Read one VMQA test-command result; refuse it without a version or switch list."""

from __future__ import annotations

import json
import sys
from pathlib import Path


def refuse(reason: str) -> int:
    print(f"refused: {reason}", file=sys.stderr)
    return 1


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: read-test-result.py <record.json>", file=sys.stderr)
        return 2
    path = Path(argv[1])
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as err:
        return refuse(f"unreadable result {path}: {err}")
    try:
        record = json.loads(raw)
    except ValueError as err:
        return refuse(f"result {path} is not valid JSON: {err}")
    if not isinstance(record, dict):
        return refuse(f"result {path} is not a JSON object")
    version = record.get("oneBuildVersion")
    if not isinstance(version, str) or not version.strip():
        return refuse(f"result {path} has no one-build version")
    switches = record.get("switches")
    if (
        not isinstance(switches, list)
        or not switches
        or not all(isinstance(switch, str) and switch.strip() for switch in switches)
    ):
        return refuse(f"result {path} has no switch list")
    print(f"accepted: version={version} switches={','.join(switches)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
