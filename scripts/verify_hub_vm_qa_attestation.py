#!/usr/bin/env python3
"""Verify the manual two-clean-VM gate for an exact signed OSL Privacy installer."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import tempfile
import unittest
from posixpath import basename
from pathlib import Path
from urllib.parse import unquote, urlparse


REQUIRED_CASES = {
    "onboarding",
    "identityCreate",
    "identityRecover",
    "twoAccountLogin",
    "persistenceRestart",
    "signedUpdate",
    "oneSidedEncryption",
    "twoSidedEncryption",
    "fullCleanup",
}
TAG = re.compile(r"^hub-v[0-9A-Za-z.+-]{1,64}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
RELEASE_DOWNLOAD_PREFIX = "/OSLPrivacy/discord-privacy-client/releases/download/"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json_object(path: Path, label: str) -> dict[str, object]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"{label} is unreadable: {error}") from error
    require(isinstance(document, dict), f"{label} must be an object")
    return document


def verify_updater_manifest(tag: str, manifest_path: Path, installer: Path) -> None:
    require(manifest_path.is_file(), "signed updater manifest is missing")
    manifest = read_json_object(manifest_path, "signed updater manifest")

    expected_version = tag.removeprefix("hub-v")
    require(manifest.get("version") == expected_version,
            "signed updater manifest version does not match the candidate tag")

    platforms = manifest.get("platforms")
    require(isinstance(platforms, dict) and platforms,
            "signed updater manifest platforms are missing")
    require(len(platforms) == 1,
            "signed updater manifest must name exactly one tested Windows artifact")

    [(platform_name, platform)] = platforms.items()
    require(isinstance(platform_name, str) and platform_name.startswith("windows-"),
            "signed updater manifest must target the Windows candidate")
    require(isinstance(platform, dict), "signed updater manifest platform entry must be an object")
    require(isinstance(platform.get("signature"), str) and bool(platform["signature"].strip()),
            "signed updater manifest platform signature is missing")

    url = platform.get("url")
    require(isinstance(url, str) and bool(url.strip()),
            "signed updater manifest platform URL is missing")
    parsed = urlparse(url)
    require(parsed.scheme == "https" and parsed.netloc == "github.com",
            "signed updater manifest must use the GitHub draft release URL")
    expected_path_prefix = f"{RELEASE_DOWNLOAD_PREFIX}{tag}/"
    require(parsed.path.startswith(expected_path_prefix),
            "signed updater manifest URL does not point at the candidate draft")
    require("/hub-latest/" not in parsed.path,
            "signed updater manifest URL must not point at the stable feed")
    require(unquote(basename(parsed.path)) == installer.name,
            "signed updater manifest URL does not name the tested installer")


def verify(tag: str, candidate_dir: Path, attestation_path: Path) -> None:
    require(bool(TAG.fullmatch(tag)), "invalid candidate tag")
    installers = sorted(candidate_dir.glob("*.exe"))
    require(len(installers) == 1, "candidate must contain exactly one Windows installer")

    document = read_json_object(attestation_path, "QA attestation")
    require(document.get("schemaVersion") == 1, "unsupported QA attestation schema")
    require(document.get("candidateTag") == tag, "QA attestation tag does not match")
    expected_hash = document.get("candidateSha256")
    require(isinstance(expected_hash, str) and bool(SHA256.fullmatch(expected_hash)),
            "QA attestation candidateSha256 is invalid")
    require(file_sha256(installers[0]) == expected_hash,
            "QA attestation does not match the exact candidate installer")
    verify_updater_manifest(tag, candidate_dir / "latest.json", installers[0])

    require(isinstance(document.get("completedAtUtc"), str) and document["completedAtUtc"].endswith("Z"),
            "QA attestation needs a UTC completion timestamp")
    require(isinstance(document.get("operator"), str) and document["operator"].strip(),
            "QA attestation needs an accountable operator")
    require(document.get("captchaHandling") == "paused_for_manual_completion",
            "CAPTCHA handling must explicitly pause for the operator")

    vms = document.get("vms")
    require(isinstance(vms, list) and len(vms) == 2, "exactly two clean VM runs are required")
    snapshot_ids: set[str] = set()
    vm_names: set[str] = set()
    for vm in vms:
        require(isinstance(vm, dict), "each VM attestation must be an object")
        name = vm.get("name")
        snapshot = vm.get("goldenSnapshotId")
        require(isinstance(name, str) and name.strip(), "each VM needs a name")
        require(isinstance(snapshot, str) and snapshot.strip(), "each VM needs a golden snapshot ID")
        require(vm.get("cleanRestore") is True, "each VM must attest a clean golden restore")
        vm_names.add(name)
        snapshot_ids.add(snapshot)
    require(len(vm_names) == 2, "the two VM names must be distinct")
    require(len(snapshot_ids) == 2, "the two golden snapshot IDs must be distinct")

    cases = document.get("cases")
    require(isinstance(cases, dict), "QA attestation cases are missing")
    require(set(cases) == REQUIRED_CASES, "QA attestation case set is incomplete or unknown")
    require(all(value is True for value in cases.values()), "every required QA case must pass")


class HubVmQaAttestationVerifierTests(unittest.TestCase):
    def candidate(self, root: Path) -> tuple[Path, Path]:
        installer = root / "osl-hub-0.1.0-x64-nsis.exe"
        installer.write_bytes(b"signed candidate fixture")
        (root / "latest.json").write_text(
            json.dumps(
                {
                    "version": "0.1.0",
                    "platforms": {
                        "windows-x86_64": {
                            "signature": "signed-update-fixture",
                            "url": (
                                "https://github.com/OSLPrivacy/discord-privacy-client/"
                                "releases/download/hub-v0.1.0/"
                                "osl-hub-0.1.0-x64-nsis.exe"
                            ),
                        }
                    },
                }
            ),
            encoding="utf-8",
        )
        attestation = root / "hub-vm-qa-attestation.json"
        attestation.write_text(
            json.dumps(
                {
                    "schemaVersion": 1,
                    "candidateTag": "hub-v0.1.0",
                    "candidateSha256": hashlib.sha256(installer.read_bytes()).hexdigest(),
                    "completedAtUtc": "2026-07-17T23:00:00Z",
                    "operator": "qa-reviewer",
                    "captchaHandling": "paused_for_manual_completion",
                    "vms": [
                        {"name": "A", "goldenSnapshotId": "signed-a", "cleanRestore": True},
                        {"name": "B", "goldenSnapshotId": "signed-b", "cleanRestore": True},
                    ],
                    "cases": {
                        "onboarding": True,
                        "identityCreate": True,
                        "identityRecover": True,
                        "twoAccountLogin": True,
                        "persistenceRestart": True,
                        "signedUpdate": True,
                        "oneSidedEncryption": True,
                        "twoSidedEncryption": True,
                        "fullCleanup": True,
                    },
                }
            ),
            encoding="utf-8",
        )
        return installer, attestation

    def test_accepts_exact_candidate_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            verify("hub-v0.1.0", root, attestation)

    def test_rejects_manifest_for_a_different_release_tag(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            manifest_path = root / "latest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["platforms"]["windows-x86_64"]["url"] = (
                "https://github.com/OSLPrivacy/discord-privacy-client/"
                "releases/download/hub-v0.1.1/osl-hub-0.1.0-x64-nsis.exe"
            )
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)

    def test_rejects_manifest_that_names_a_different_installer(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            manifest_path = root / "latest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["platforms"]["windows-x86_64"]["url"] = (
                "https://github.com/OSLPrivacy/discord-privacy-client/"
                "releases/download/hub-v0.1.0/other-installer.exe"
            )
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tag", required=True)
    parser.add_argument("--candidate-dir", required=True, type=Path)
    parser.add_argument("--attestation", required=True, type=Path)
    args = parser.parse_args()
    verify(args.tag, args.candidate_dir, args.attestation)
    print("OK: exact signed candidate passed the two-clean-VM attestation gate")


if __name__ == "__main__":
    main()
