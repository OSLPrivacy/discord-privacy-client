#!/usr/bin/env python3
"""Prove the unchanged TASK 5188 check goes red for each ruled defect."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts/task-5188-check.py"
MUTANTS = {
    "empty-inventory": "empty inventory location=writer_inventory count=0",
    "omitted-writer": "omitted writer location=relay_held_undelivered_ciphertexts count=1",
    "retained-item": "retained discovered item location=service_settings count=1",
    "receipt-divergence": "receipt divergence location=post_cleanup_storage receipt_count=1 scan_count=0",
    "deleted-tombstone": "deleted tombstone location=lost_key_name_tombstones count=0",
}


def run(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(CHECK), *args],
        cwd=ROOT,
        env=os.environ.copy(),
        check=False,
        capture_output=True,
        text=True,
    )


def main() -> int:
    green = run([])
    if green.returncode != 0:
        sys.stderr.write(green.stdout + green.stderr)
        return 1
    print("TASK5188 mutants unchanged_tree=green")
    for fault, diagnostic in MUTANTS.items():
        result = run(["--fault", fault])
        output = result.stdout + result.stderr
        if result.returncode != 1 or diagnostic not in output:
            print(
                f"TASK5188 FAIL mutant={fault} exit={result.returncode} expected_diagnostic={diagnostic!r}\n{output}",
                file=sys.stderr,
            )
            return 1
        print(f"TASK5188 mutant={fault} exit=1 diagnostic={diagnostic}")
    print(f"TASK5188 MUTANTS PASS red={len(MUTANTS)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
