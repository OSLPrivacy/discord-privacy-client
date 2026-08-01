#!/usr/bin/env python3
"""Refuse Rust toolchain-pin drift and print the released-tag toolchain map.

The configured CI default is not, by itself, proof of the compiler Cargo uses:
rustup applies rust-toolchain.toml from the checked-out tree.  This checker makes
both facts explicit and compares the recorded Windows runner observation too.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


class PinError(RuntimeError):
    pass


def channel(path: Path) -> str:
    match = re.search(r'^channel\s*=\s*"([^"]+)"\s*$', path.read_text(), re.MULTILINE)
    if not match:
        raise PinError(f"{path}: missing [toolchain].channel")
    return match.group(1)


def workflow_pin(path: Path) -> str:
    text = path.read_text()
    match = re.search(
        r"uses:\s+dtolnay/rust-toolchain@[^\n]+\n\s+with:\s*\n(?:\s*#.*\n)*\s+toolchain:\s*['\"]?([^\s'\"]+)",
        text,
    )
    if not match:
        raise PinError(f"{path}: missing dtolnay/rust-toolchain toolchain input")
    return match.group(1)


def observed_release(path: Path) -> str:
    match = re.search(r"^release:\s*([^\s]+)\s*$", path.read_text(), re.MULTILINE)
    if not match:
        raise PinError(f"{path}: missing recorded rustc -vV release")
    return match.group(1)


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args], text=True, capture_output=True, check=False
    )
    if result.returncode:
        raise PinError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout


def tag_map(root: Path) -> dict[str, str]:
    tags = [tag for tag in git(root, "tag", "--list", "hub-v*").splitlines() if tag]
    if not tags:
        raise PinError("no hub-v* tags available for the reproducibility map")
    mapped: dict[str, str] = {}
    for tag in tags:
        contents = git(root, "show", f"{tag}:rust-toolchain.toml")
        match = re.search(r'^channel\s*=\s*"([^"]+)"\s*$', contents, re.MULTILINE)
        if not match:
            raise PinError(f"{tag}:rust-toolchain.toml: missing [toolchain].channel")
        mapped[tag] = match.group(1)
    return mapped


def check(root: Path, observation: Path) -> tuple[str, dict[str, str]]:
    pins = {
        "rust-toolchain.toml": channel(root / "rust-toolchain.toml"),
        "rust-test.yml": workflow_pin(root / ".github/workflows/rust-test.yml"),
        "osl-hub-release.yml": workflow_pin(root / ".github/workflows/osl-hub-release.yml"),
        "reproducible-build.yml": workflow_pin(root / ".github/workflows/reproducible-build.yml"),
    }
    expected = pins["rust-toolchain.toml"]
    mismatched = [f"{name}={value}" for name, value in pins.items() if value != expected]
    if mismatched:
        raise PinError("toolchain pins disagree: " + ", ".join(mismatched))

    observed = observed_release(observation)
    if observed != expected:
        raise PinError(f"observed rustc release {observed} does not match configured pin {expected}")

    # A future current-pin bump must not invalidate a released tag: its map is
    # deliberately informational so T8 can keep rebuilding that historic tree.
    return expected, tag_map(root)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--observation",
        type=Path,
        default=Path("docs/release/toolchain-pin-map.md"),
        help="file containing the rustc -vV output captured from a Windows CI run",
    )
    args = parser.parse_args()
    root = args.root.resolve()
    observation = args.observation if args.observation.is_absolute() else root / args.observation
    try:
        pin, mapped = check(root, observation)
    except (OSError, PinError) as error:
        print(f"toolchain pin check failed: {error}", file=sys.stderr)
        return 1
    print(f"configured and observed rustc release: {pin}")
    for tag, value in sorted(mapped.items()):
        print(f"{tag}\t{value}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
