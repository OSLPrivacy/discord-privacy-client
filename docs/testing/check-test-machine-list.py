#!/usr/bin/env python3
"""Fail closed when a starting QA machine-list entry is incomplete."""
import json
import sys
from pathlib import Path

REQUIRED = ("name", "system", "location", "intended_use")

def main(path: str) -> int:
    entries = json.loads(Path(path).read_text())['machines']
    errors = []
    for index, entry in enumerate(entries):
        for field in REQUIRED:
            if not isinstance(entry.get(field), str) or not entry[field].strip():
                errors.append(f"entry {index} missing {field}")
    if errors:
        print("; ".join(errors), file=sys.stderr)
        return 1
    print(f"machine_list_entries={len(entries)}")
    print(f"complete_entries={len(entries)}")
    return 0

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("usage: check-test-machine-list.py LIST.json", file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
