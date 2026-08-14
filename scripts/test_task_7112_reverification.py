#!/usr/bin/env python3
"""Focused acceptance checks for TASK 7112's batch caller."""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import run_task_7112_reverification as sweep


ROOT = Path(__file__).resolve().parents[1]
PLAN = Path("/home/liamw/osl-plan/OSL-AUDITS")
SCRIPT = ROOT / "scripts" / "run_task_7112_reverification.py"


def expect_red(args: list[str], needle: str) -> None:
    result = subprocess.run([sys.executable, str(SCRIPT), *args], cwd=ROOT, text=True, capture_output=True)
    assert result.returncode == 1, (args, result.stdout, result.stderr)
    assert needle in result.stderr, (needle, result.stderr)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="task-7112-inputs-") as directory:
        root = Path(directory)
        todo = root / "todo"
        rejected = root / "evidence" / "rejected"
        todo.mkdir(parents=True)
        rejected.mkdir(parents=True)
        for name in sweep.TARGETS:
            shutil.copy2(PLAN / "todo" / name, todo / name)
        shutil.copy2(PLAN / "evidence" / "rejected" / "0381.md", rejected / "0381.md")
        before = sweep.target_plan_fingerprint(root)
        (todo / sweep.TARGETS[0]).write_text((todo / sweep.TARGETS[0]).read_text() + "\nmutation\n")
        assert before != sweep.target_plan_fingerprint(root)
        before = sweep.fingerprint(rejected)
        (rejected / "0381.md").write_text((rejected / "0381.md").read_text() + "\nmutation\n")
        assert before != sweep.fingerprint(rejected)
    for item in ("control set", "design-page check", "stale index", "before-and-after comparison"):
        expect_red(["--starve", item], f"absent input: {item}")
    ledger = ROOT / "proof" / "reverification" / "verdict-ledger.json"
    document = json.loads(ledger.read_text(encoding="utf-8"))["sweeps"]["task-7112-onboarding-redesign"]
    rows = document["rows"]
    assert len(rows) == len({row["taskId"] for row in rows}) == 278
    assert document["counts"]["FAILS"] > 0 or document["counts"]["CANNOT-DECIDE-MECHANICALLY"] > 0
    for task_id in ("0319", "0357", "0377", "0434", "0456e", "6804"):
        row = next(row for row in rows if row["taskId"] == task_id)
        assert row["verdict"] == "FAILS" and row["clause"]
    assert not any(row["taskId"] == "0381" for row in rows)
    screens = [row["screen"] for row in rows if "screen" in row]
    assert screens and all("design_page" in row for row in screens)
    print("TASK7112_TEST result=passed rows=278 separate_counts=1 seeded_fails=6 rejected_owner=1 screen_rows=%d no_reference=%d plan_mutation_red=1 rejected_mutation_red=1 starvation=4" % (len(screens), sum(row["design_page"] is None for row in screens)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
