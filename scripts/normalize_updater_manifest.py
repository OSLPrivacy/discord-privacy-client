#!/usr/bin/env python3
"""Rewrite the tauri-action updater manifest into the shape the promotion gate accepts.

`tauri-action` uploads a `latest.json` that names the same artifact twice — once under the
canonical `windows-x86_64` key the Tauri updater looks up, and once under the bundle-suffixed
`windows-x86_64-nsis` alias — and points both at `api.github.com/.../releases/assets/<id>`.
That asset endpoint only resolves for a caller holding a token that can see the draft, so it is
useless to an installed client, and `scripts/verify_hub_vm_qa_attestation.py` rejects both
properties. The two halves of the release pipeline were therefore mutually unreachable.

This script is the reconciliation, and it moves the manifest toward the verifier — never the
other way round. It collapses the aliases onto the single canonical platform key and rewrites the
URL to the public `releases/download/<tag>/<asset>` form that an installed client can actually
fetch. It refuses, loudly, to discard an alias that names a *different* artifact.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

CANONICAL_PLATFORM = "windows-x86_64"
RELEASE_DOWNLOAD_TEMPLATE = (
    "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/{tag}/{asset}"
)
TAG_PREFIX = "hub-v"
# Fields tauri-action emits that are preserved verbatim. `notes` is the release-body text the
# app renders in its update dialog; rewriting it here would hide it from review.
PRESERVED_TOP_LEVEL = ("version", "notes", "pub_date")


class NormalizationError(Exception):
    """The emitted manifest cannot be normalized without losing or inventing information."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise NormalizationError(message)


def normalize_manifest(manifest: Any, tag: str, installer_name: str) -> dict[str, Any]:
    """Return the promotion-gate shape of ``manifest`` for ``tag`` and ``installer_name``."""
    _require(isinstance(manifest, dict), "updater manifest must be a JSON object")
    _require(tag.startswith(TAG_PREFIX), f"candidate tag must start with {TAG_PREFIX!r}")
    _require(
        installer_name.endswith(".exe") and "/" not in installer_name,
        "installer asset name must be a bare Windows .exe file name",
    )

    expected_version = tag.removeprefix(TAG_PREFIX)
    _require(
        manifest.get("version") == expected_version,
        "updater manifest version does not match the candidate tag",
    )

    platforms = manifest.get("platforms")
    _require(
        isinstance(platforms, dict) and bool(platforms),
        "updater manifest has no platform entries to normalize",
    )

    windows = {
        name: entry for name, entry in platforms.items()
        if isinstance(name, str) and name.startswith("windows-")
    }
    _require(bool(windows), "updater manifest names no Windows artifact")
    _require(
        CANONICAL_PLATFORM in windows,
        f"updater manifest is missing the canonical {CANONICAL_PLATFORM} entry the "
        "Tauri updater looks up",
    )

    canonical = windows[CANONICAL_PLATFORM]
    _require(isinstance(canonical, dict), "canonical platform entry must be an object")

    signature = canonical.get("signature")
    _require(
        isinstance(signature, str) and bool(signature.strip()),
        "canonical platform entry carries no updater signature",
    )

    # Every alias we are about to drop must be the same artifact under a different name. A
    # differing signature means a genuinely distinct bundle, and silently discarding it would
    # publish a feed that omits a built artifact.
    for name, entry in sorted(windows.items()):
        if name == CANONICAL_PLATFORM:
            continue
        _require(isinstance(entry, dict), f"platform entry {name!r} must be an object")
        _require(
            entry.get("signature") == signature,
            f"platform alias {name!r} signs a different artifact than {CANONICAL_PLATFORM!r}; "
            "refusing to drop it",
        )

    normalized: dict[str, Any] = {
        key: manifest[key] for key in PRESERVED_TOP_LEVEL if key in manifest
    }
    normalized["platforms"] = {
        CANONICAL_PLATFORM: {
            "signature": signature,
            "url": RELEASE_DOWNLOAD_TEMPLATE.format(tag=tag, asset=installer_name),
        }
    }
    return normalized


def normalize_file(path: Path, tag: str, installer_name: str, output: Path) -> dict[str, Any]:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise NormalizationError(f"updater manifest is unreadable: {error}") from error
    normalized = normalize_manifest(manifest, tag, installer_name)
    output.write_text(json.dumps(normalized, indent=2) + "\n", encoding="utf-8")
    return normalized


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument(
        "--installer-name",
        required=True,
        help="the published .exe asset name, as uploaded to the draft release",
    )
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    output = args.output if args.output is not None else args.manifest
    try:
        normalized = normalize_file(args.manifest, args.tag, args.installer_name, output)
    except NormalizationError as error:
        raise SystemExit(f"updater manifest normalization refused: {error}") from error
    url = normalized["platforms"][CANONICAL_PLATFORM]["url"]
    print(f"OK: normalized updater manifest to a single {CANONICAL_PLATFORM} entry at {url}")


if __name__ == "__main__":
    main()
