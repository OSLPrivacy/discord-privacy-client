#!/usr/bin/env python3
"""Validate the mandatory impact manifest for an accepted semantic change."""
from __future__ import annotations
import argparse, json

RULES = {
    "product-intent": ("master", "layman"),
    "implementation-evidence": ("master", "checklist"),
    "dependency-owner-blocker": ("dag", "checklist", "telegram"),
    "security": ("master", "checklist", "telegram", "public-claim"),
    "durable-design": ("design-guide", "tokens", "components", "assets", "screenshots", "tests"),
    "deadline": ("master-2.1", "checklist", "telegram-eta", "priority", "unresolved-deadline-memory"),
}

class ChangeSetError(ValueError): pass

def validate(manifest: dict) -> None:
    for key in ("change_id", "classification", "semantic_change", "authorities", "affected_artifacts"):
        if key not in manifest: raise ChangeSetError(f"missing {key}")
    if not manifest["semantic_change"]:
        return
    classifications = manifest["classification"]
    if not classifications: raise ChangeSetError("semantic change needs a classification")
    entries = set(manifest["authorities"])
    artifacts = set(manifest["affected_artifacts"])
    for kind in classifications:
        if kind not in RULES: raise ChangeSetError(f"unknown classification: {kind}")
        missing = set(RULES[kind]) - entries
        if missing: raise ChangeSetError(f"{kind} missing required entries: {', '.join(sorted(missing))}")
    if not artifacts: raise ChangeSetError("semantic change needs affected docs/tests/copy artifacts")
    memory = manifest.get("compact_memory", {})
    if memory.get("field_changed") and not memory.get("update"):
        raise ChangeSetError("changed compact-memory field needs an update")
    if "product-intent" not in classifications and "layman" not in entries and not manifest.get("layman_update_not_due"):
        raise ChangeSetError("state why the layman view is not due")

def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("manifest")
    args = parser.parse_args()
    try:
        with open(args.manifest, encoding="utf-8") as fh: validate(json.load(fh))
    except (OSError, json.JSONDecodeError, ChangeSetError) as exc:
        print(f"change-set refused: {exc}"); return 1
    return 0
if __name__ == "__main__": raise SystemExit(main())
