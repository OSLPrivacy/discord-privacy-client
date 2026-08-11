#!/usr/bin/env python3
"""Run TASK 5188's executable backend boundary check."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
TARGET = "/mnt/d/osl-lane-targets/i"
FAULTS = {
    "empty-inventory",
    "omitted-writer",
    "retained-item",
    "receipt-divergence",
    "deleted-tombstone",
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fault", choices=sorted(FAULTS))
    args = parser.parse_args()
    env = os.environ.copy()
    if env.get("CARGO_TARGET_DIR") != TARGET:
        print(
            f"TASK5188 FAIL CARGO_TARGET_DIR={env.get('CARGO_TARGET_DIR')!r} expected={TARGET!r}",
            file=sys.stderr,
        )
        return 1
    if args.fault:
        env["TASK5188_FAULT"] = args.fault
    else:
        env.pop("TASK5188_FAULT", None)
    result = subprocess.run(
        [
            "cargo",
            "test",
            "-p",
            "ipc",
            "--test",
            "task_5188_delete_account_cleanup",
            "--",
            "--test-threads=1",
            "--nocapture",
        ],
        cwd=ROOT,
        env=env,
        check=False,
        text=True,
    )
    return 0 if result.returncode == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
