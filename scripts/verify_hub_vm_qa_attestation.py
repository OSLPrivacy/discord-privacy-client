#!/usr/bin/env python3
"""Verify the manual two-clean-VM gate for an exact signed OSL Privacy installer."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


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


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify(tag: str, candidate_dir: Path, attestation_path: Path) -> None:
    require(bool(TAG.fullmatch(tag)), "invalid candidate tag")
    installers = sorted(candidate_dir.glob("*.exe"))
    require(len(installers) == 1, "candidate must contain exactly one Windows installer")
    require((candidate_dir / "latest.json").is_file(), "signed updater manifest is missing")

    try:
        document = json.loads(attestation_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"QA attestation is unreadable: {error}") from error
    require(isinstance(document, dict), "QA attestation must be an object")
    require(document.get("schemaVersion") == 1, "unsupported QA attestation schema")
    require(document.get("candidateTag") == tag, "QA attestation tag does not match")
    expected_hash = document.get("candidateSha256")
    require(isinstance(expected_hash, str) and bool(SHA256.fullmatch(expected_hash)),
            "QA attestation candidateSha256 is invalid")
    require(file_sha256(installers[0]) == expected_hash,
            "QA attestation does not match the exact candidate installer")

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
