from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from scripts.verify_hub_vm_qa_attestation import verify


def write_candidate(root: Path) -> tuple[Path, Path]:
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
                "finalApprover": "qa-final-approver-second-session",
                "packageReproducedBySecondSession": True,
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


class HubVmQaAttestationTests(unittest.TestCase):
    def candidate(self, root: Path) -> tuple[Path, Path]:
        return write_candidate(root)

    def test_accepts_exact_candidate_and_complete_two_vm_gate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            verify("hub-v0.1.0", root, attestation)

    def test_rejects_installer_changed_after_qa(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            installer, attestation = self.candidate(root)
            installer.write_bytes(b"different candidate")
            with self.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)

    def test_rejects_incomplete_test_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            document = json.loads(attestation.read_text(encoding="utf-8"))
            document["cases"].pop("fullCleanup")
            attestation.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)


def freeze_the_exact_signed_candidate_vm_attestation_contract() -> None:
    testcase = unittest.TestCase()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        installer, attestation = write_candidate(root)
        manifest_path = root / "latest.json"

        verify("hub-v0.1.0", root, attestation)

        invalid_documents = []
        document = json.loads(attestation.read_text(encoding="utf-8"))
        for key, value in (
            ("candidateTag", "hub-v0.1.1"),
            ("candidateSha256", "0" * 64),
            ("completedAtUtc", "2026-07-17T23:00:00"),
            ("finalApprover", document["operator"]),
            ("packageReproducedBySecondSession", False),
            ("captchaHandling", "automated"),
        ):
            mutated = dict(document)
            mutated[key] = value
            invalid_documents.append(mutated)

        mutated_vms = dict(document)
        mutated_vms["vms"] = [
            {"name": "A", "goldenSnapshotId": "signed-a", "cleanRestore": True},
            {"name": "A", "goldenSnapshotId": "signed-b", "cleanRestore": True},
        ]
        invalid_documents.append(mutated_vms)

        mutated_cases = dict(document)
        mutated_cases["cases"] = dict(document["cases"])
        mutated_cases["cases"].pop("fullCleanup")
        invalid_documents.append(mutated_cases)

        for invalid_document in invalid_documents:
            attestation.write_text(json.dumps(invalid_document), encoding="utf-8")
            with testcase.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)

        attestation.write_text(json.dumps(document), encoding="utf-8")
        installer.write_bytes(b"different candidate")
        with testcase.assertRaises(SystemExit):
            verify("hub-v0.1.0", root, attestation)
        installer.write_bytes(b"signed candidate fixture")

        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["platforms"]["windows-x86_64"]["url"] = (
            "https://github.com/OSLPrivacy/discord-privacy-client/"
            "releases/download/hub-latest/osl-hub-0.1.0-x64-nsis.exe"
        )
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        with testcase.assertRaises(SystemExit):
            verify("hub-v0.1.0", root, attestation)


freeze_the_exact_signed_candidate_vm_attestation_contract.__name__ = (
    "Freeze the exact signed-candidate VM attestation contract."
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, pattern
    tests.addTest(unittest.FunctionTestCase(
        freeze_the_exact_signed_candidate_vm_attestation_contract,
    ))
    return tests


if __name__ == "__main__":
    unittest.main()
