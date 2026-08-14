#!/usr/bin/env python3
"""Executable coverage contract for TASK 3204's short security review pack."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PACK = ROOT / "docs/security/osl-security-review-pack.md"
REGISTRY = ROOT / "data/pricing.json"


def load_registry() -> list[dict[str, object]]:
    data = json.loads(REGISTRY.read_text(encoding="utf-8"))
    rows = data.get("capability_registry")
    if not isinstance(rows, list) or not rows:
        raise ValueError("capability_registry is absent or empty")
    return rows


def validate(text: str, registry: list[dict[str, object]]) -> list[str]:
    errors: list[str] = []
    normalized = re.sub(r"\s+", " ", text).strip()

    table_rows = re.findall(
        r"^\| `([^`]+)` \| ([^|]+?) \| (Beta|Planned|Illustration) \|",
        text,
        flags=re.MULTILINE,
    )
    actual = {(row_id, name.strip(), status) for row_id, name, status in table_rows}
    expected = {
        (str(row["id"]), str(row["name"]), str(row["status"]))
        for row in registry
    }
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    if missing:
        errors.append(f"missing website capability rows: {missing}")
    if extra:
        errors.append(f"unexpected website capability rows: {extra}")
    if len(table_rows) != len(actual):
        errors.append("duplicate website capability rows")

    required_phrases = {
        "screen-photo risk": "screen photo",
        "malware risk": "malware",
        "provider-account risk": "provider-account risk",
        "recipient adversary": "malicious or cooperating recipient",
        "provider adversary": "connected service",
        "passive recorder adversary": "passive recorder",
        "no current adapter claim": "marks every protected adapter unavailable",
        "not an outside review": "It is not an outside review",
        "no verified release": "site has no verified release",
        "prototype non-claim": "Both public prototypes are simulations",
        "process representations": "process and commercial representations",
    }
    lowered = normalized.lower()
    for label, phrase in required_phrases.items():
        if phrase.lower() not in lowered:
            errors.append(f"missing {label}: {phrase!r}")

    limitation_bullets = re.findall(r"^- (.+(?:\n  .+)*)", text, re.MULTILINE)
    if len(limitation_bullets) < 8:
        errors.append(
            f"open limitations need at least 8 explicit bullets; found {len(limitation_bullets)}"
        )

    beta_rows = [row for row in registry if row.get("status") == "Beta"]
    if len(beta_rows) != 2:
        errors.append(f"expected exactly 2 Beta registry rows; found {len(beta_rows)}")
    if "source-level confidentiality claim, not a claim that a downloadable release" not in normalized:
        errors.append("source proof is not explicitly separated from release proof")
    if "Not accepted for the current release" not in text:
        errors.append("historical protected-text QA is not refused as current-release proof")
    if "wire_v2.rs:685-760" not in text or "pqxdh.rs:141-195" not in text:
        errors.append("the affirmative cryptographic source claim lacks both proof anchors")

    forbidden_affirmations = [
        r"\bcurrent release is proved\b",
        r"\bevery protected adapter (?:is|works)\b",
        r"\bOSL protects (?:groups|files|images)\b",
        r"\bprevents (?:screenshots|screen photos|malware|account bans)\b",
        r"\b(?:has been|is) independently reviewed\b",
    ]
    for pattern in forbidden_affirmations:
        if re.search(pattern, text, flags=re.IGNORECASE):
            errors.append(f"unsupported affirmative claim matches {pattern!r}")

    static_surface_markers = [
        "how it works:",
        "download:",
        "faq:",
        "pricing and terms:",
        "donate:",
        "compare:",
        "your text protection is ready",
        "nine launch companions",
        "three sensitive-history findings",
        "local old-address preview",
        "42 checks",
        "11 cleaned links",
        "zero verified removals",
        "server-plaintext retention promise",
        "chats lab's e2ee messages",
    ]
    missing_static = [marker for marker in static_surface_markers if marker not in lowered]
    if missing_static:
        errors.append(f"missing static website statements: {missing_static}")

    return errors


def self_test(text: str, registry: list[dict[str, object]]) -> list[str]:
    mutants = {
        "missing-screen-photo": text.replace("screen photo", "camera picture", 1),
        "missing-capability": re.sub(
            r"^\| `autoscrub` \|.*\n", "", text, count=1, flags=re.MULTILINE
        ),
        "status-promotion": text.replace(
            "| `burn` | Burn | Planned |", "| `burn` | Burn | Beta |", 1
        ),
        "false-release-proof": text + "\nThe current release is proved.\n",
    }
    survivors = [name for name, mutant in mutants.items() if not validate(mutant, registry)]
    return survivors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pack", type=Path, default=DEFAULT_PACK)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    registry = load_registry()
    text = args.pack.read_text(encoding="utf-8")
    errors = validate(text, registry)
    status_counts: dict[str, int] = {}
    for row in registry:
        status = str(row["status"])
        status_counts[status] = status_counts.get(status, 0) + 1

    print(f"TASK3204 website_claims={len(registry)}")
    print(
        "TASK3204 statuses="
        + ",".join(f"{key}:{status_counts[key]}" for key in sorted(status_counts))
    )
    print("TASK3204 named_risks=screen-photo,malware,provider-account")
    print(f"TASK3204 unsupported_affirmative_claims={len(errors)}")

    if args.self_test:
        survivors = self_test(text, registry)
        print(f"TASK3204 self_test_mutants=4 killed={4 - len(survivors)}")
        if survivors:
            print("TASK3204 surviving_mutants=" + ",".join(survivors))
            return 1

    if errors:
        for error in errors:
            print(f"TASK3204 ERROR: {error}")
        return 1
    print("TASK3204 review_pack=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
