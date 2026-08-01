#!/usr/bin/env python3
"""Refuse a release when its manifest, Rust package, and UI package versions drift.

Tauri's generated context uses ``tauri.conf.json``'s ``version`` when it is
present, so ``app.package_info().version`` reports that value at runtime.  The
release tag is also derived from this same manifest.  Cargo and the UI package
must therefore match it before a release can continue.
"""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path


class VersionConsistencyError(Exception):
    """A version declaration is missing, invalid, or does not agree."""


def _read_version(path: Path, loader: object, label: str, section: str | None = None) -> str:
    try:
        with path.open("rb") as source:
            document = loader(source)
    except (OSError, UnicodeError, json.JSONDecodeError, tomllib.TOMLDecodeError) as error:
        raise VersionConsistencyError(f"cannot read {label} at {path}: {error}") from error

    if section is not None and isinstance(document, dict):
        document = document.get(section)
    version = document.get("version") if isinstance(document, dict) else None
    if not isinstance(version, str) or not version:
        raise VersionConsistencyError(f"{label} at {path} has no string version")
    return version


def versions(root: Path) -> dict[str, str]:
    """Read every release version declaration below ``root``."""
    return {
        "tauri.conf.json (runtime and release tag)": _read_version(
            root / "apps/osl-hub/tauri.conf.json", json.load, "tauri.conf.json"
        ),
        "Cargo.toml": _read_version(
            root / "apps/osl-hub/Cargo.toml", tomllib.load, "Cargo.toml", "package"
        ),
        "package.json": _read_version(
            root / "apps/osl-hub-ui/package.json", json.load, "package.json"
        ),
    }


def check(root: Path) -> str:
    """Return the shared version, or raise with every declaration on mismatch."""
    declared = versions(root)
    if len(set(declared.values())) != 1:
        details = ", ".join(f"{label}={version}" for label, version in declared.items())
        raise VersionConsistencyError(f"OSL Privacy version declarations disagree: {details}")
    return next(iter(declared.values()))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (defaults to this script's repository)",
    )
    args = parser.parse_args()
    try:
        version = check(args.root)
    except VersionConsistencyError as error:
        print(f"version consistency check failed: {error}", file=sys.stderr)
        return 1
    print(f"version consistency check passed: {version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
