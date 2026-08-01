#!/usr/bin/env python3
"""Create a deterministic SHA256SUMS file and optionally minisign it with Tauri's signer."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
from pathlib import Path
from typing import Sequence


def _validated_assets(output: Path, assets: Sequence[Path]) -> list[Path]:
    if not assets:
        raise SystemExit("at least one release asset is required")
    resolved_output = output.resolve()
    seen_names: set[str] = set()
    validated: list[Path] = []
    for asset in assets:
        if not asset.is_file():
            raise SystemExit(f"release asset is not a regular file: {asset}")
        if asset.resolve() == resolved_output:
            raise SystemExit("checksum output cannot also be a release asset")
        if asset.name != Path(asset.name).name:
            raise SystemExit(f"release asset must have a basename: {asset}")
        if asset.name in seen_names:
            raise SystemExit(f"release asset names must be unique: {asset.name}")
        seen_names.add(asset.name)
        validated.append(asset)
    return sorted(validated, key=lambda asset: asset.name)


def write_checksums(output: Path, assets: Sequence[Path]) -> None:
    """Write POSIX sha256sum-format records, ordered by release asset name."""
    records = []
    for asset in _validated_assets(output, assets):
        digest = hashlib.sha256(asset.read_bytes()).hexdigest()
        records.append(f"{digest}  {asset.name}\n")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("".join(records), encoding="utf-8", newline="\n")


def sign_checksums(checksums: Path, signer: str) -> Path:
    """Sign with the Tauri updater key supplied through the standard environment variables."""
    subprocess.run([signer, "signer", "sign", str(checksums)], check=True)
    tauri_signature = checksums.with_name(f"{checksums.name}.sig")
    minisign_signature = checksums.with_name(f"{checksums.name}.minisig")
    if not tauri_signature.is_file():
        raise SystemExit(f"Tauri signer did not create {tauri_signature}")
    tauri_signature.replace(minisign_signature)
    return minisign_signature


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--sign",
        action="store_true",
        help="sign the completed checksum list using TAURI_SIGNING_PRIVATE_KEY",
    )
    parser.add_argument("--signer", default="tauri", help="Tauri CLI executable")
    parser.add_argument("assets", nargs="+", type=Path)
    args = parser.parse_args()

    write_checksums(args.output, args.assets)
    if args.sign:
        signature = sign_checksums(args.output, args.signer)
        print(f"wrote {args.output} and {signature}")
    else:
        print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
