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

    def test_accepts_exact_candidate_and_complete_two_vm_gate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            verify("hub-v0.1.0", root, attestation)

    def test_rejects_stable_feed_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            manifest_path = root / "latest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["platforms"]["windows-x86_64"]["url"] = (
                "https://github.com/OSLPrivacy/discord-privacy-client/"
                "releases/download/hub-latest/osl-hub-0.1.0-x64-nsis.exe"
            )
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)

    def test_rejects_manifest_for_untested_installer_name(self) -> None:
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

    def test_rejects_installer_changed_after_qa(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            installer, attestation = self.candidate(root)
            installer.write_bytes(b"different candidate")
            with self.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)

    def test_rejects_missing_second_session_reproduction(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            document = json.loads(attestation.read_text(encoding="utf-8"))
            document["packageReproducedBySecondSession"] = False
            attestation.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "second session"):
                verify("hub-v0.1.0", root, attestation)

    def test_rejects_same_session_final_approver(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            document = json.loads(attestation.read_text(encoding="utf-8"))
            document["finalApprover"] = document["operator"]
            attestation.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "different session"):
                verify("hub-v0.1.0", root, attestation)

    def test_rejects_non_manual_captcha_handling(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            document = json.loads(attestation.read_text(encoding="utf-8"))
            document["captchaHandling"] = "automated"
            attestation.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaises(SystemExit):
                verify("hub-v0.1.0", root, attestation)

    def test_rejects_reused_vm_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, attestation = self.candidate(root)
            document = json.loads(attestation.read_text(encoding="utf-8"))
            document["vms"][1]["goldenSnapshotId"] = document["vms"][0]["goldenSnapshotId"]
            attestation.write_text(json.dumps(document), encoding="utf-8")
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
    test_case = HubVmQaAttestationTests()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        installer, attestation = test_case.candidate(root)

        verify("hub-v0.1.0", root, attestation)

        document = json.loads(attestation.read_text(encoding="utf-8"))
        manifest_path = root / "latest.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

        document["candidateSha256"] = hashlib.sha256(b"untested package").hexdigest()
        attestation.write_text(json.dumps(document), encoding="utf-8")
        with test_case.assertRaises(SystemExit):
            verify("hub-v0.1.0", root, attestation)
        document["candidateSha256"] = hashlib.sha256(installer.read_bytes()).hexdigest()

        document["packageReproducedBySecondSession"] = False
        attestation.write_text(json.dumps(document), encoding="utf-8")
        with test_case.assertRaises(SystemExit):
            verify("hub-v0.1.0", root, attestation)
        document["packageReproducedBySecondSession"] = True

        document["finalApprover"] = document["operator"]
        attestation.write_text(json.dumps(document), encoding="utf-8")
        with test_case.assertRaises(SystemExit):
            verify("hub-v0.1.0", root, attestation)
        document["finalApprover"] = "qa-final-approver-second-session"
        attestation.write_text(json.dumps(document), encoding="utf-8")

        manifest["platforms"]["windows-x86_64"]["url"] = (
            "https://github.com/OSLPrivacy/discord-privacy-client/"
            "releases/download/hub-latest/osl-hub-0.1.0-x64-nsis.exe"
        )
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        with test_case.assertRaises(SystemExit):
            verify("hub-v0.1.0", root, attestation)


freeze_the_exact_signed_candidate_vm_attestation_contract.__name__ = (
    "Freeze the exact signed-candidate VM attestation contract."
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del tests, pattern
    suite = unittest.TestSuite()
    suite.addTests(loader.loadTestsFromTestCase(HubVmQaAttestationTests))
    suite.addTest(unittest.FunctionTestCase(
        freeze_the_exact_signed_candidate_vm_attestation_contract,
    ))
    return suite


if __name__ == "__main__":
    unittest.main()
