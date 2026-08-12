#!/usr/bin/env python3
"""Prove TASK 4708's verifier fails for every prohibited shortcut."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
VERIFY = ROOT / "verify.py"


def run(artifacts: Path, allowance: Path) -> int:
    return subprocess.run([sys.executable, str(VERIFY), "--artifacts", str(artifacts), "--allowance", str(allowance)], capture_output=True, text=True).returncode


def edit_jsonl(path: Path, edit) -> None:
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    for row in rows:
        if row.get("event") == "interface_counters": edit(row)
    path.write_text("\n".join(json.dumps(row, sort_keys=True) for row in rows) + "\n")


def main() -> int:
    artifacts, allowance = map(Path, sys.argv[1:3])
    failures = []
    mutations = {
        "starve-transmit": lambda a, r: edit_jsonl(a / "speaker-A.jsonl", lambda row: row.update(sent_bytes=0)),
        "starve-receive": lambda a, r: edit_jsonl(a / "speaker-A.jsonl", lambda row: row.update(received_bytes=0)),
        "synthetic-counters": lambda a, r: edit_jsonl(a / "speaker-A.jsonl", lambda row: row.update(counter_source="synthetic")),
        "bypass-accounting": lambda a, r: r["people"][0].update(interface_sent_bytes=0),
        "force-voice-row-zero": lambda a, r: next(row for row in r["rows"] if row["class"] == "voice").update(bytes=0),
    }
    for name, mutate in mutations.items():
        with tempfile.TemporaryDirectory(prefix="task4708-mutant-") as temporary:
            root = Path(temporary)
            copied_artifacts = root / "artifacts"
            shutil.copytree(artifacts, copied_artifacts)
            report = json.loads(allowance.read_text())
            mutate(copied_artifacts, report)
            copied_allowance = root / "allowance.json"
            copied_allowance.write_text(json.dumps(report))
            code = run(copied_artifacts, copied_allowance)
            print(f"TASK4708_MUTANT name={name} exit={code}")
            if code != 1: failures.append(name)
    restored = run(artifacts, allowance)
    print(f"TASK4708_RESTORED exit={restored}")
    if failures or restored != 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
