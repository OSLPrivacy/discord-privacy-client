from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from scripts.verify_hub_vm_qa_attestation import verify


class HubVmQaAttestationTests(unittest.TestCase):
    def candidate(self, root: Path) -> tuple[Path, Path]:
        installer = root / "osl-hub-0.1.0-x64-nsis.exe"
        installer.write_bytes(b"signed candidate fixture")
        (root / "latest.json").write_text("{}", encoding="utf-8")
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


if __name__ == "__main__":
    unittest.main()
