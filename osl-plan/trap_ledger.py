#!/usr/bin/env python3
"""Keep high-return traps narrow, evidence-backed, and reviewable."""
from __future__ import annotations
import argparse, json, re
REQUIRED=("id","location","context","symptom","actual_cause","failed_obvious_approach","discriminating_check","safe_invariant","evidence","last_verified")
ID=re.compile(r"^TRAP-[A-Z0-9]+-[0-9]+$")
class TrapError(ValueError): pass
def validate(entries: list[dict]) -> None:
    signatures=set()
    for entry in entries:
        missing=[key for key in REQUIRED if not entry.get(key)]
        if missing: raise TrapError("trap missing: "+", ".join(missing))
        if not ID.fullmatch(entry['id']): raise TrapError(f"invalid TRAP-ID: {entry['id']}")
        if entry.get('status','active') not in ('active','superseded'): raise TrapError('invalid status')
        sig=(entry['location'],entry['actual_cause'].strip().lower())
        if sig in signatures and entry.get('status','active') != 'superseded': raise TrapError(f"duplicate broad trap at {entry['location']}")
        signatures.add(sig)
def check_placement(master: str, subsystem_table: str) -> None:
    if 'TRAP-' in master and subsystem_table.strip(): raise TrapError('cross-file runtime trap belongs in the subsystem table, not master')
def main()->int:
    p=argparse.ArgumentParser(); p.add_argument('ledger'); p.add_argument('--master'); p.add_argument('--subsystem-table'); a=p.parse_args()
    try:
        with open(a.ledger,encoding='utf-8') as f: validate(json.load(f))
        if a.master and a.subsystem_table: check_placement(open(a.master,encoding='utf-8').read(),open(a.subsystem_table,encoding='utf-8').read())
    except (OSError,json.JSONDecodeError,TrapError) as e: print('trap-ledger refused:',e);return 1
    return 0
if __name__=='__main__':raise SystemExit(main())
