#!/usr/bin/env python3
"""Pre-implementation validator for owner-authorized architecture changes."""
from __future__ import annotations
import argparse, importlib.util, json
from pathlib import Path
REQUIRED = {"owner_wording", "date", "affected_ids", "supersession", "impact_map", "contract", "dag", "views", "compatibility", "owner_notice", "change_set", "feature_views"}
IMPACT_AREAS = {"ui", "native", "crypto-wire", "persistence-migration", "network-deploy", "consent", "pricing-tier", "test-matrix", "docs-site", "telemetry", "rollback", "dependencies", "current-tasks"}
COMPATIBILITY = {"transition", "downgrade_resistance", "rollback", "old_data", "removal_date"}
class SpecChangeError(ValueError): pass
def _load(name: str, relative: str):
    spec=importlib.util.spec_from_file_location(name, Path(__file__).resolve().parents[1]/relative)
    module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module); return module
def validate(packet: dict) -> None:
    missing=REQUIRED-set(packet)
    if missing: raise SpecChangeError("missing packet fields: "+", ".join(sorted(missing)))
    if not packet["owner_wording"] or not packet["date"] or not packet["affected_ids"]: raise SpecChangeError("dated owner wording and affected IDs are required")
    unsuperseded=[claim for claim, state in packet["supersession"].items() if state != "superseded"]
    if unsuperseded: raise SpecChangeError("unsuperseded prior claims: "+", ".join(unsuperseded))
    missing=IMPACT_AREAS-set(packet["impact_map"])
    if missing: raise SpecChangeError("impact map missing: "+", ".join(sorted(missing)))
    for key in ("acceptance", "defaults", "migration"):
        if not packet["contract"].get(key): raise SpecChangeError(f"contract missing {key}")
    if not packet["dag"].get("recalculated") or not packet["dag"].get("eta_current"): raise SpecChangeError("DAG/ETA is stale")
    if not packet["views"].get("synchronized"): raise SpecChangeError("product views are not synchronized")
    missing=COMPATIBILITY-set(packet["compatibility"])
    if missing: raise SpecChangeError("compatibility missing: "+", ".join(sorted(missing)))
    try:
        _load("change_set", "osl-plan/change_set.py").validate(packet["change_set"])
        sync=_load("feature_sync", "scripts/check-feature-view-sync.py")
        sync.reconcile(**packet["feature_views"])
    except ValueError as error: raise SpecChangeError(str(error)) from error
def main() -> int:
    p=argparse.ArgumentParser(); p.add_argument("packet"); a=p.parse_args()
    try:
        with open(a.packet, encoding="utf-8") as f: validate(json.load(f))
    except (OSError, json.JSONDecodeError, SpecChangeError) as e: print('spec-change refused:', e); return 1
    return 0
if __name__=='__main__': raise SystemExit(main())
