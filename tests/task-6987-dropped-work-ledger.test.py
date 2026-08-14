#!/usr/bin/env python3
"""Focused black-box test for the re-runnable TASK 6987 ledger."""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/task-6987-dropped-work-ledger.py"
LANES = [f"lane/{letter}" for letter in "bcdefghijklmnopqrstuvwxyz"]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="task-6987-") as directory:
        output = Path(directory) / "ledger.json"
        command = [sys.executable, str(SCRIPT),
                   "--deferred-log", str(ROOT / "tests/task-6987-deferred-lane-edits.txt"),
                   "--reachability-input", str(ROOT / "tests/task-6987-reachability.md"),
                   "--disposition-rules", str(ROOT / "tests/task-6987-dispositions.json"),
                   "--integration", "integration/2026-08-09", "--rc", "rc/48h",
                   "--lanes", *LANES, "--output", str(output)]
        completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
        if completed.returncode:
            raise SystemExit(completed.stderr)
        report = json.loads(output.read_text())
        assert report["deferred_summary"] == {"rows": 1, "lines": 55, "merge_dispositions": {"keep RC": 1}}
        row = report["deferred"][0]
        assert row["lane_blob"]["object"] and row["integration_base_blob"]["object"]
        source = row["lane_blob"]["content"] + row["integration_base_blob"]["content"]
        for required in ("Nobody is whitelisted yet", "Found session", "Encrypted attachments"):
            assert required in source, required
        # Census evidence is generated from refs, not a seeded symbol list.
        assert report["census"]["symbols"]
        assert all("definitions" in entry and "calls" in entry for entry in report["census"]["symbols"])
        assert report["reachability"] and all(row["disposition"] in {"restored", "superseded", "open"}
                                               for row in report["reachability"])
        print("TASK 6987 focused ledger test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
