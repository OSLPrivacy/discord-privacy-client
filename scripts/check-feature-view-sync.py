#!/usr/bin/env python3
"""Refuse feature-view drift; every stable ID must occur in all three views."""
from __future__ import annotations
import argparse, re
LINE = re.compile(r"^\s*([A-Za-z][A-Za-z0-9_-]*)\s*(?:\||:)\s*([A-Za-z][A-Za-z0-9 _-]*)\s*$")

def read_view(path: str) -> dict[str, str]:
    found = {}
    with open(path, encoding="utf-8") as file:
        for number, line in enumerate(file, 1):
            match = LINE.match(line)
            if match:
                feature, status = match.groups()
                if feature in found: raise ValueError(f"{path}:{number}: duplicate stable ID {feature}")
                found[feature] = status.strip().lower()
    return found

def reconcile(master: dict[str,str], layman: dict[str,str], checklist: dict[str,str]) -> None:
    views = {"master": master, "layman": layman, "internal-checklist": checklist}
    all_ids = set().union(*views.values())
    errors=[]
    for feature in sorted(all_ids):
        present=[name for name, view in views.items() if feature in view]
        if len(present) != 3:
            errors.append(f"{feature}: missing from {', '.join(sorted(set(views)-set(present)))}")
            continue
        statuses={view[feature] for view in views.values()}
        if len(statuses) != 1: errors.append(f"{feature}: status mismatch ({', '.join(f'{n}={v[feature]}' for n,v in views.items())})")
    if errors: raise ValueError("feature views out of sync: " + "; ".join(errors))

def main() -> int:
    p=argparse.ArgumentParser(); p.add_argument("master"); p.add_argument("layman"); p.add_argument("checklist"); a=p.parse_args()
    try: reconcile(read_view(a.master), read_view(a.layman), read_view(a.checklist))
    except (OSError, ValueError) as error: print(error); return 1
    return 0
if __name__ == "__main__": raise SystemExit(main())
