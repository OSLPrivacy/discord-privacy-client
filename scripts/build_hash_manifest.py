#!/usr/bin/env python3
"""Create and sign the build-hash record for one OSL Privacy release."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any


SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)*$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
HUB_EXECUTABLE = "osl-privacy-hub.exe"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def sha256_file(path: Path) -> str:
    require(path.is_file(), f"release artifact is not a regular file: {path}")
    digest = hashlib.sha256()
    with path.open("rb") as artifact:
        for chunk in iter(lambda: artifact.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def executable_sha256_from_installer(installer: Path, archive_tool: str) -> str:
    """Extract the PE from the NSIS installer and return its SHA-256 digest."""
    require(installer.is_file(), f"installer is not a regular file: {installer}")
    with tempfile.TemporaryDirectory() as temporary:
        extracted = Path(temporary)
        try:
            subprocess.run(
                [archive_tool, "x", str(installer), f"-o{extracted}", "-y"],
                check=True,
            )
        except (OSError, subprocess.CalledProcessError) as error:
            raise SystemExit(f"could not extract installer with {archive_tool}: {error}") from error
        executables = [path for path in extracted.rglob(HUB_EXECUTABLE) if path.is_file()]
        require(
            len(executables) == 1,
            f"installer must contain exactly one {HUB_EXECUTABLE}, got {len(executables)}",
        )
        return sha256_file(executables[0])


def build_manifest(
    *, version: str, tag: str, commit: str, installer: Path, archive_tool: str
) -> dict[str, Any]:
    require(bool(SEMVER.fullmatch(version)), "version must be a semantic version without a leading v")
    require(tag == f"hub-v{version}", "tag must exactly match the hub-v release version")
    require(bool(COMMIT.fullmatch(commit)), "commit must be a full lowercase 40-hex Git commit")
    installer_sha256 = sha256_file(installer)
    executable_sha256 = executable_sha256_from_installer(installer, archive_tool)
    return {
        "format": 1,
        "builds": [{
            "version": version,
            "tag": tag,
            "commit": commit,
            "installer_sha256": installer_sha256,
            "exe_sha256": executable_sha256,
        }],
    }


def write_manifest(output: Path, manifest: dict[str, Any]) -> bytes:
    payload = (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode("utf-8")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(payload)
    return payload


def sign_manifest(manifest: Path, signer: str) -> Path:
    subprocess.run([signer, "signer", "sign", str(manifest)], check=True)
    signature = manifest.with_name(f"{manifest.name}.sig")
    require(signature.is_file(), f"Tauri signer did not create {signature}")
    return signature


def verify_manifest_artifacts(manifest: dict[str, Any], installer: Path, archive_tool: str) -> None:
    """Fail if the manifest is not bound to the installer and PE it names."""
    require(manifest.get("format") == 1, "build-hash manifest format is unsupported")
    builds = manifest.get("builds")
    require(isinstance(builds, list) and len(builds) == 1, "build-hash manifest must contain one build")
    build = builds[0]
    require(isinstance(build, dict), "build-hash manifest build must be an object")
    for field in ("installer_sha256", "exe_sha256"):
        value = build.get(field)
        require(isinstance(value, str) and bool(SHA256.fullmatch(value)), f"{field} must be lowercase SHA-256")
    require(build["installer_sha256"] == sha256_file(installer), "installer SHA-256 does not match manifest")
    require(
        build["exe_sha256"] == executable_sha256_from_installer(installer, archive_tool),
        "executable SHA-256 does not match manifest",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--installer", required=True, type=Path)
    parser.add_argument("--archive-tool", default="7z")
    parser.add_argument("--sign", action="store_true")
    parser.add_argument("--signer", default="tauri")
    args = parser.parse_args()

    manifest = build_manifest(
        version=args.version,
        tag=args.tag,
        commit=args.commit,
        installer=args.installer,
        archive_tool=args.archive_tool,
    )
    write_manifest(args.output, manifest)
    verify_manifest_artifacts(manifest, args.installer, args.archive_tool)
    if args.sign:
        signature = sign_manifest(args.output, args.signer)
        print(f"wrote {args.output} and {signature}")
    else:
        print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
