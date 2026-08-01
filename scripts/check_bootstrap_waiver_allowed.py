#!/usr/bin/env python3
"""Refuse a bootstrap QA waiver once the update feed it was excused for actually exists.

`signedUpdate` is waivable exactly once, because the first promotion is the one that creates the
`hub-latest` feed the case needs. That excuse is only true while the feed is empty. This guard is
the half of the waiver the promotion workflow can check against reality: it reads the live
`hub-latest` asset list and fails closed if a waiver is presented after a `latest.json` has
already been published. The exception therefore expires on its own, without anyone remembering to
withdraw it.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

FEED_MANIFEST_ASSET = "latest.json"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def waived_cases(attestation: Any) -> frozenset[str]:
    require(isinstance(attestation, dict), "QA attestation must be an object")
    cases = attestation.get("cases")
    require(isinstance(cases, dict), "QA attestation cases are missing")
    return frozenset(
        name for name, value in cases.items()
        if isinstance(value, str) and value.startswith("waived:")
    )


def feed_carries_manifest(feed: Any) -> bool:
    """True when the live `hub-latest` release already publishes an updater manifest.

    Accepts either the `gh release view --json assets` object or the bare asset list, and treats
    a missing release (`null`, or the empty string the workflow passes when `gh` fails) as "no
    feed yet" — which is exactly the bootstrap condition.
    """
    if feed is None:
        return False
    assets = feed.get("assets") if isinstance(feed, dict) else feed
    if assets is None:
        return False
    require(isinstance(assets, list), "hub-latest asset list must be a JSON array")
    return any(
        isinstance(asset, dict) and asset.get("name") == FEED_MANIFEST_ASSET
        for asset in assets
    )


def check(attestation: Any, feed: Any) -> frozenset[str]:
    waived = waived_cases(attestation)
    if not waived:
        return frozenset()
    require(
        not feed_carries_manifest(feed),
        "bootstrap QA waiver refused: hub-latest already publishes "
        f"{FEED_MANIFEST_ASSET}, so {sorted(waived)} must be attested for real",
    )
    return waived


def _load(path: Path, label: str) -> Any:
    try:
        text = path.read_text(encoding="utf-8").strip()
    except (OSError, UnicodeError) as error:
        raise SystemExit(f"{label} is unreadable: {error}") from error
    if not text:
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError as error:
        raise SystemExit(f"{label} is not valid JSON: {error}") from error


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--attestation", required=True, type=Path)
    parser.add_argument(
        "--feed-assets",
        required=True,
        type=Path,
        help="`gh release view hub-latest --json assets` output, or an empty file if absent",
    )
    args = parser.parse_args()

    waived = check(_load(args.attestation, "QA attestation"), _load(args.feed_assets, "hub-latest assets"))
    if waived:
        print(f"OK: bootstrap waiver accepted for {sorted(waived)} — no update feed is published yet")
    else:
        print("OK: no QA case is waived")


if __name__ == "__main__":
    main()
