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

def compare_azure(path: str, machine_list: str) -> int:
    """Require exactly one running Azure row per newly reserved region."""
    rows = json.loads(Path(path).read_text())
    machines = json.loads(Path(machine_list).read_text())["machines"]
    regions = {"francecentral", "polandcentral", "italynorth", "norwayeast"}
    errors = []
    for region in sorted(regions):
        matches = [row for row in rows if row.get("location") == region]
        if len(matches) != 1:
            errors.append(f"{region}: expected 1 row, got {len(matches)}")
            continue
        row = matches[0]
        if row.get("powerState") != "VM running":
            errors.append(f"{region}: machine is not running")
        listed = [m for m in machines if m.get("location") == f"Azure {region}"]
        if len(listed) != 1:
            errors.append(f"{region}: expected 1 machine-list row, got {len(listed)}")
        elif listed[0].get("processor_count") != row.get("processorCount"):
            errors.append(f"{region}: processor count mismatch")
    if errors:
        print("; ".join(errors), file=sys.stderr)
        return 1
    print(f"azure_regions_checked={len(regions)}")
    print("azure_processor_counts=" + ",".join(f"{r}:{next(x['processorCount'] for x in rows if x['location']==r)}" for r in sorted(regions)))
    return 0

if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--azure":
        raise SystemExit(compare_azure(sys.argv[2], sys.argv[3]))
    if len(sys.argv) != 2:
        print("usage: check-test-machine-list.py LIST.json | --azure AZURE.json LIST.json", file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(main(sys.argv[1]))
