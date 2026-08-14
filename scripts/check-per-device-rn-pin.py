#!/usr/bin/env python3
"""Gate OSL-RN shipping on per-device-pair downgrade pins."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


FAILURE = "per-device pin required"


def fail() -> int:
    sys.stdout.write(f"{FAILURE}\n")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".", help="repository root to inspect")
    args = parser.parse_args()

    root = Path(args.root)
    wire_rn = root / "crates" / "ipc" / "src" / "wire_rn.rs"
    commands = root / "crates" / "ipc" / "src" / "commands.rs"
    try:
        wire = wire_rn.read_text(encoding="utf-8")
        command_source = commands.read_text(encoding="utf-8")
    except OSError:
        return fail()

    required = [
        "fn device_pair_key(",
        "pub fn load_pair_pin(",
        "pub fn raise_pair_pin_to_rn(",
        ".load_pair_pin(",
        ".raise_pair_pin_to_rn(",
    ]
    if any(token not in wire for token in required[:3]):
        return fail()
    if required[3] not in command_source:
        return fail()
    if required[4] not in wire:
        return fail()

    per_peer_pin_shapes = [
        r"fn\s+pin_path\s*\(\s*&self\s*,\s*peer_identity_x25519\s*:",
        r"fn\s+pin_floor_path\s*\(\s*&self\s*,\s*peer_identity_x25519\s*:",
        r"b\"OSL-RN/v1/pin-file/\"",
    ]
    if any(re.search(pattern, wire) for pattern in per_peer_pin_shapes):
        return fail()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
