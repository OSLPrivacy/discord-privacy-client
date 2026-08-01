"""The release↔promote manifest contract, exercised against the real published manifest.

The fixture at `scripts/fixtures/hub-v0.1.0-tauri-action-latest.json` is a byte-for-byte copy of
the `latest.json` that `tauri-action` actually uploaded to the `hub-v0.1.0` release. Its sha256 is
asserted below against the digest GitHub recorded for that asset, so this is the emitted artifact
and not a reconstruction of it.

Every assertion here runs the real gate — `verify_hub_vm_qa_attestation`, the same module
`osl-hub-promote.yml` invokes — over real files on disk. Nothing greps source text.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from check_bootstrap_waiver_allowed import check as check_bootstrap_waiver
from normalize_updater_manifest import NormalizationError, normalize_file, normalize_manifest
from verify_hub_vm_qa_attestation import BOOTSTRAP_WAIVER_VALUE, verify, verify_updater_manifest

EMITTED_MANIFEST = SCRIPTS / "fixtures" / "hub-v0.1.0-tauri-action-latest.json"
# GitHub's recorded digest for the `latest.json` asset on the hub-v0.1.0 release.
EMITTED_MANIFEST_SHA256 = "33ecf2a2d54e5749b8cc610866cc2eb8c4d7d05a86a46681580a3a3e960d10bb"
TAG = "hub-v0.1.0"
INSTALLER_NAME = "osl-hub-0.1.0-x64-nsis.exe"
VERIFIER = SCRIPTS / "verify_hub_vm_qa_attestation.py"


def attestation_document(installer: Path, *, waive_signed_update: bool) -> dict[str, object]:
    cases: dict[str, object] = {
        "onboarding": True,
        "identityCreate": True,
        "identityRecover": True,
        "twoAccountLogin": True,
        "persistenceRestart": True,
        "signedUpdate": True,
        "oneSidedEncryption": True,
        "twoSidedEncryption": True,
        "fullCleanup": True,
    }
    document: dict[str, object] = {
        "schemaVersion": 1,
        "candidateTag": TAG,
        "candidateSha256": hashlib.sha256(installer.read_bytes()).hexdigest(),
        "completedAtUtc": "2026-07-31T23:00:00Z",
        "operator": "qa-reviewer",
        "finalApprover": "qa-final-approver-second-session",
        "packageReproducedBySecondSession": True,
        "captchaHandling": "paused_for_manual_completion",
        "vms": [
            {"name": "A", "goldenSnapshotId": "golden-a", "cleanRestore": True},
            {"name": "B", "goldenSnapshotId": "golden-b", "cleanRestore": True},
        ],
        "cases": cases,
    }
    if waive_signed_update:
        cases["signedUpdate"] = BOOTSTRAP_WAIVER_VALUE
        document["bootstrapWaiver"] = {
            "appliesToTag": TAG,
            "cases": ["signedUpdate"],
            "approvedBy": "release-owner",
            "reason": (
                "The first promotion is the one that creates the hub-latest feed, so no "
                "installed build can take a signed update from it beforehand."
            ),
            "restoredByTag": "hub-v0.1.1",
        }
    return document


class ReleaseManifestContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        # The verifier hashes the installer against the attestation's own record, so its bytes
        # only have to be stable; the manifest is what is under test.
        self.installer = self.root / INSTALLER_NAME
        self.installer.write_bytes(b"candidate installer bytes")

    def emitted(self) -> dict[str, object]:
        return json.loads(EMITTED_MANIFEST.read_text(encoding="utf-8"))

    def write_attestation(self, document: dict[str, object]) -> Path:
        path = self.root / "hub-vm-qa-attestation.json"
        path.write_text(json.dumps(document), encoding="utf-8")
        return path

    def test_fixture_is_the_manifest_github_actually_serves(self) -> None:
        digest = hashlib.sha256(EMITTED_MANIFEST.read_bytes()).hexdigest()
        self.assertEqual(digest, EMITTED_MANIFEST_SHA256)

    # --- the deadlock, stated as a test -------------------------------------------------

    def test_emitted_manifest_names_two_platforms_and_an_api_url(self) -> None:
        """The two properties the gate refuses, read off the real artifact."""
        platforms = self.emitted()["platforms"]
        self.assertEqual(len(platforms), 2)
        for entry in platforms.values():
            self.assertIn("api.github.com", entry["url"])

    def test_gate_rejects_the_emitted_manifest_for_the_duplicate_platform(self) -> None:
        manifest = self.root / "latest.json"
        manifest.write_bytes(EMITTED_MANIFEST.read_bytes())
        with self.assertRaisesRegex(SystemExit, "exactly one tested Windows artifact"):
            verify_updater_manifest(TAG, manifest, self.installer)

    def test_gate_rejects_the_emitted_manifest_for_the_api_url(self) -> None:
        """With the duplicate removed the URL is still refused — two independent defects."""
        document = self.emitted()
        document["platforms"].pop("windows-x86_64-nsis")
        manifest = self.root / "latest.json"
        manifest.write_text(json.dumps(document), encoding="utf-8")
        with self.assertRaisesRegex(SystemExit, "GitHub draft release URL"):
            verify_updater_manifest(TAG, manifest, self.installer)

    # --- the fix ------------------------------------------------------------------------

    def test_normalized_manifest_satisfies_the_verifier_module(self) -> None:
        manifest = self.root / "latest.json"
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, manifest)
        verify_updater_manifest(TAG, manifest, self.installer)

    def test_normalization_preserves_the_updater_signature_and_notes(self) -> None:
        normalized = normalize_manifest(self.emitted(), TAG, INSTALLER_NAME)
        emitted = self.emitted()
        self.assertEqual(
            normalized["platforms"]["windows-x86_64"]["signature"],
            emitted["platforms"]["windows-x86_64"]["signature"],
        )
        self.assertEqual(normalized["notes"], emitted["notes"])
        self.assertEqual(normalized["version"], emitted["version"])
        self.assertEqual(
            normalized["platforms"]["windows-x86_64"]["url"],
            "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/"
            f"{TAG}/{INSTALLER_NAME}",
        )

    def test_normalization_refuses_to_drop_an_alias_naming_a_different_artifact(self) -> None:
        document = self.emitted()
        document["platforms"]["windows-x86_64-nsis"]["signature"] = "a-genuinely-different-bundle"
        with self.assertRaisesRegex(NormalizationError, "refusing to drop it"):
            normalize_manifest(document, TAG, INSTALLER_NAME)

    def test_normalization_refuses_a_tag_that_does_not_match_the_manifest_version(self) -> None:
        with self.assertRaisesRegex(NormalizationError, "does not match the candidate tag"):
            normalize_manifest(self.emitted(), "hub-v0.9.9", INSTALLER_NAME)

    # --- the real promotion gate, end to end, as a subprocess ---------------------------

    def run_verifier(self, attestation: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable, str(VERIFIER),
                "--tag", TAG,
                "--candidate-dir", str(self.root),
                "--attestation", str(attestation),
            ],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_promote_gate_accepts_a_normalized_candidate_end_to_end(self) -> None:
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, self.root / "latest.json")
        attestation = self.write_attestation(
            attestation_document(self.installer, waive_signed_update=False)
        )
        result = self.run_verifier(attestation)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("passed the two-clean-VM attestation gate", result.stdout)

    def test_promote_gate_rejects_the_unnormalized_candidate_end_to_end(self) -> None:
        (self.root / "latest.json").write_bytes(EMITTED_MANIFEST.read_bytes())
        attestation = self.write_attestation(
            attestation_document(self.installer, waive_signed_update=False)
        )
        result = self.run_verifier(attestation)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exactly one tested Windows artifact", result.stderr)

    # --- the bootstrap waiver -----------------------------------------------------------

    def test_bootstrap_waiver_lets_the_first_promotion_through(self) -> None:
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, self.root / "latest.json")
        attestation = self.write_attestation(
            attestation_document(self.installer, waive_signed_update=True)
        )
        result = self.run_verifier(attestation)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_waiver_is_refused_for_a_case_that_is_not_bootstrap_blocked(self) -> None:
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, self.root / "latest.json")
        document = attestation_document(self.installer, waive_signed_update=True)
        document["cases"]["fullCleanup"] = BOOTSTRAP_WAIVER_VALUE
        document["bootstrapWaiver"]["cases"] = ["signedUpdate", "fullCleanup"]
        attestation = self.write_attestation(document)
        with self.assertRaisesRegex(SystemExit, "bootstrap-waivable"):
            verify(TAG, self.root, attestation)

    def test_waiver_bound_to_another_tag_is_refused(self) -> None:
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, self.root / "latest.json")
        document = attestation_document(self.installer, waive_signed_update=True)
        document["bootstrapWaiver"]["appliesToTag"] = "hub-v0.1.1"
        attestation = self.write_attestation(document)
        with self.assertRaisesRegex(SystemExit, "bound to this candidate tag"):
            verify(TAG, self.root, attestation)

    def test_waiver_without_a_named_approver_is_refused(self) -> None:
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, self.root / "latest.json")
        document = attestation_document(self.installer, waive_signed_update=True)
        document["bootstrapWaiver"]["approvedBy"] = "   "
        attestation = self.write_attestation(document)
        with self.assertRaisesRegex(SystemExit, "named accountable approver"):
            verify(TAG, self.root, attestation)

    def test_waived_case_without_a_waiver_record_is_refused(self) -> None:
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, self.root / "latest.json")
        document = attestation_document(self.installer, waive_signed_update=True)
        document.pop("bootstrapWaiver")
        attestation = self.write_attestation(document)
        with self.assertRaisesRegex(SystemExit, "requires a bootstrapWaiver record"):
            verify(TAG, self.root, attestation)

    def test_a_plain_false_case_is_still_refused(self) -> None:
        normalize_file(EMITTED_MANIFEST, TAG, INSTALLER_NAME, self.root / "latest.json")
        document = attestation_document(self.installer, waive_signed_update=False)
        document["cases"]["signedUpdate"] = False
        attestation = self.write_attestation(document)
        with self.assertRaisesRegex(SystemExit, "every required QA case must pass"):
            verify(TAG, self.root, attestation)

    # --- the waiver expires against reality, not against a promise ----------------------

    def test_waiver_is_refused_once_the_feed_publishes_a_manifest(self) -> None:
        document = attestation_document(self.installer, waive_signed_update=True)
        live_feed = {"assets": [{"name": "SHA256SUMS.txt"}, {"name": "latest.json"}]}
        with self.assertRaisesRegex(SystemExit, "already publishes latest.json"):
            check_bootstrap_waiver(document, live_feed)

    def test_waiver_is_allowed_while_the_feed_carries_no_manifest(self) -> None:
        document = attestation_document(self.installer, waive_signed_update=True)
        # This is the live hub-latest asset list as of the bootstrap: checksums only.
        self.assertEqual(
            check_bootstrap_waiver(document, {"assets": [{"name": "SHA256SUMS.txt"}]}),
            frozenset({"signedUpdate"}),
        )
        self.assertEqual(
            check_bootstrap_waiver(document, None),
            frozenset({"signedUpdate"}),
        )

    def test_unwaived_attestation_is_unaffected_by_a_live_feed(self) -> None:
        document = attestation_document(self.installer, waive_signed_update=False)
        self.assertEqual(
            check_bootstrap_waiver(document, {"assets": [{"name": "latest.json"}]}),
            frozenset(),
        )


if __name__ == "__main__":
    unittest.main()
