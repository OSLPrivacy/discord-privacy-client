#!/usr/bin/env python3
"""Mutation tests for the fail-closed C4 shipping evidence verifier."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
import tempfile
import time
import unittest
import uuid
from pathlib import Path


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
RELEASE_REPORT = ROOT / "docs" / "reports" / "release-lane-2026-07-26.md"
SPEC = importlib.util.spec_from_file_location(
    "c4_shipping_evidence",
    HERE / "c4-shipping-evidence.py",
)
assert SPEC and SPEC.loader
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)


def sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


class ShippingEvidenceMutationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="osl-c4-shipping-")
        self.root = Path(self.temporary.name)
        self.target = "Deckard QA"
        self.executable = self.root / "osl-privacy-hub.exe"
        self.executable.write_bytes(b"MZ\x00shipping desktop fixture\x00")
        self.screenshot = self.root / "after.png"
        self.screenshot.write_bytes(b"\x89PNG\r\n\x1a\nfixture")
        now = time.time_ns() // 1_000_000
        self.start = now - 1_000
        self.end = now + 1_000
        os.utime(self.screenshot, ns=(now * 1_000_000, now * 1_000_000))
        run_id = str(uuid.uuid4())
        executable_sha = sha(self.executable.read_bytes())
        carrier_sha = sha(b"shipping carrier")
        binding = sha(b"signed Discord process|window|Deckard QA")
        self.bundle = {
            "schema": VERIFIER.SCHEMA,
            "runId": run_id,
            "runStartUnixMs": self.start,
            "runEndUnixMs": self.end,
            "targetConversation": self.target,
            "build": {
                "frontendCommand": "npm --prefix apps/osl-hub-ui run build",
                "cargoCommand": (
                    "osl-cargo -C apps/osl-hub build --release "
                    "--features desktop --target x86_64-pc-windows-msvc"
                ),
                "cargoFeatures": ["desktop"],
                "qaShell": False,
                "observedAtUnixMs": self.start - 100,
                "executableSha256": executable_sha,
            },
            "executable": {
                "path": str(self.executable.resolve()),
                "sha256": executable_sha,
                "processId": 4102,
                "processStartedAtUnixMs": self.start + 50,
            },
            "commandReceipt": {
                "runId": run_id,
                "executableSha256": executable_sha,
                "targetConversation": self.target,
                "observedAtUnixMs": self.start + 400,
                "source": "production-overlay-ui",
                "receiptAuthority": "shipping-renderer-success-gate",
                "uiControlAutomationId": "prepare-protected",
                "backendCommand": "send_native_discord_overlay_carrier",
                "status": "sent",
                "placed": True,
                "enterSent": True,
                "qaShell": False,
                "rendererStatus": VERIFIER.SUCCESS_STATUS,
            },
            "preEnterReadback": {
                "runId": run_id,
                "executableSha256": executable_sha,
                "targetConversation": self.target,
                "observedAtUnixMs": self.start + 300,
                "authority": "native-pre-enter-exact-readback",
                "relation": "rawExact",
                "readCount": 1,
                "utf8Bytes": len(b"shipping carrier"),
                "composerTextSha256": carrier_sha,
                "exact": True,
            },
            "postEnterComposer": {
                "runId": run_id,
                "executableSha256": executable_sha,
                "targetConversation": self.target,
                "observedAtUnixMs": self.start + 500,
                "readCount": 1,
                "utf8Bytes": 0,
                "composerTextSha256": sha(b""),
                "empty": True,
            },
            "conversationRows": {
                "before": {
                    "runId": run_id,
                    "executableSha256": executable_sha,
                    "targetConversation": self.target,
                    "observedAtUnixMs": self.start + 100,
                    "namedConversationMatches": 1,
                    "transcriptMatches": 1,
                    "readCount": 1,
                    "rowCount": 7,
                    "targetBindingSha256": binding,
                },
                "after": {
                    "runId": run_id,
                    "executableSha256": executable_sha,
                    "targetConversation": self.target,
                    "observedAtUnixMs": self.start + 600,
                    "namedConversationMatches": 1,
                    "transcriptMatches": 1,
                    "readCount": 1,
                    "rowCount": 8,
                    "targetBindingSha256": binding,
                },
                "newRows": [
                    {
                        "targetConversation": self.target,
                        "targetBindingSha256": binding,
                        "carrierTextSha256": carrier_sha,
                        "rowIdentitySha256": sha(b"unique row identity"),
                        "matchCount": 1,
                    }
                ],
            },
            "screenshot": {
                "path": self.screenshot.name,
                "sha256": sha(self.screenshot.read_bytes()),
                "observedAtUnixMs": self.start + 700,
                "targetConversation": self.target,
                "namedConversationMatches": 1,
                "newRowMatches": 1,
            },
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, value: dict | None = None) -> Path:
        path = self.root / "bundle.json"
        path.write_text(
            json.dumps(self.bundle if value is None else value, separators=(",", ":")),
            encoding="utf-8",
        )
        return path

    def reject(self, mutation) -> str:
        changed = copy.deepcopy(self.bundle)
        mutation(changed)
        with self.assertRaises(VERIFIER.EvidenceError) as raised:
            VERIFIER.verify_bundle(self.write(changed), self.target)
        return str(raised.exception)

    def test_positive_fixture_reaches_every_required_seam(self) -> None:
        verdict = VERIFIER.verify_bundle(self.write(), self.target)
        self.assertEqual(verdict["verdict"], "pass")
        self.assertEqual(verdict["rowDelta"], 1)
        self.assertEqual(verdict["productionReceipt"], "sent/placed/enterSent")
        self.assertEqual(verdict["preEnterReadback"], "rawExact")
        self.assertEqual(verdict["postEnterComposer"], "empty")

    def test_qa_shell_feature_is_rejected(self) -> None:
        reason = self.reject(lambda value: value["build"].update(qaShell=True))
        self.assertIn("QA-shell", reason)

    def test_qa_shell_binary_marker_is_rejected_even_with_matching_hash(self) -> None:
        marked = self.executable.read_bytes() + b"send_native_discord_qa_atomic_text"
        self.executable.write_bytes(marked)
        marked_sha = sha(marked)

        def mutate(value):
            value["build"]["executableSha256"] = marked_sha
            value["executable"]["sha256"] = marked_sha
            for label in ("commandReceipt", "preEnterReadback", "postEnterComposer"):
                value[label]["executableSha256"] = marked_sha
            for label in ("before", "after"):
                value["conversationRows"][label]["executableSha256"] = marked_sha

        reason = self.reject(mutate)
        self.assertIn("forbidden QA-shell marker", reason)

    def test_qa_atomic_command_receipt_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["commandReceipt"].update(
                backendCommand="send_native_discord_qa_atomic_text"
            )
        )
        self.assertIn("shipping production evidence", reason)

    def test_stale_receipt_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["commandReceipt"].update(
                observedAtUnixMs=value["runStartUnixMs"] - 1
            )
        )
        self.assertIn("outside", reason)

    def test_append_only_or_stale_trail_is_not_an_accepted_field(self) -> None:
        reason = self.reject(
            lambda value: value.update(
                sendStageTrail="osl-discord-qa-send-stage.txt"
            )
        )
        self.assertIn("extra=", reason)

    def test_missing_row_evidence_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["conversationRows"].update(newRows=[])
        )
        self.assertIn("exactly one row", reason)

    def test_duplicate_row_evidence_is_rejected(self) -> None:
        def mutate(value):
            row = copy.deepcopy(value["conversationRows"]["newRows"][0])
            value["conversationRows"]["newRows"].append(row)

        reason = self.reject(mutate)
        self.assertIn("exactly one row", reason)

    def test_duplicate_named_conversation_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["conversationRows"]["after"].update(
                namedConversationMatches=2
            )
        )
        self.assertIn("one unique named conversation", reason)

    def test_wrong_target_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["commandReceipt"].update(
                targetConversation="Wrong DM"
            )
        )
        self.assertIn("wrong target", reason)

    def test_more_than_one_new_row_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["conversationRows"]["after"].update(rowCount=9)
        )
        self.assertIn("exactly one row", reason)

    def test_non_exact_pre_enter_readback_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["preEnterReadback"].update(
                relation="canonicalised", exact=False
            )
        )
        self.assertIn("byte-exact", reason)

    def test_nonempty_post_enter_composer_is_rejected(self) -> None:
        reason = self.reject(
            lambda value: value["postEnterComposer"].update(
                utf8Bytes=3,
                composerTextSha256=sha(b"bad"),
                empty=False,
            )
        )
        self.assertIn("not empty", reason)

    def test_missing_screenshot_is_rejected(self) -> None:
        self.screenshot.unlink()
        with self.assertRaisesRegex(VERIFIER.EvidenceError, "missing"):
            VERIFIER.verify_bundle(self.write(), self.target)

    def test_duplicate_json_key_is_rejected_before_semantics(self) -> None:
        path = self.write()
        raw = path.read_text(encoding="utf-8")
        raw = raw.replace(
            '"schema":"osl-c4-shipping-evidence-v1"',
            '"schema":"osl-c4-shipping-evidence-v1","schema":"forged"',
            1,
        )
        path.write_text(raw, encoding="utf-8")
        with self.assertRaisesRegex(VERIFIER.EvidenceError, "duplicate JSON key"):
            VERIFIER.verify_bundle(path, self.target)


class ShippingHarnessSourceContractTests(unittest.TestCase):
    @staticmethod
    def assert_contract(source: str) -> None:
        required = (
            "[ValidateSet('BuildShipping', 'DriveApprovedSend', 'VerifyEvidence')]",
            "Assert-LiveDriveApproval",
            "if (-not $ConfirmOwnerApprovedSend)",
            "Invoke-Checked 'osl-cargo'",
            "'--features', 'desktop'",
            "Invoke-UiaButton $send",
            "'protected-draft'",
            "'prepare-protected'",
            "Get-DiscordSnapshot $discord $ExpectedConversation",
            "Get-UiaValue $before.Composer",
            "Find-ExactCarrierInRow",
            "$discord.Root.SetFocus()",
            "Save-DiscordScreenshot",
            "'--bundle', $bundlePath",
        )
        for needle in required:
            if needle not in source:
                raise AssertionError(f"shipping harness seam is absent: {needle}")
        forbidden_live_mechanisms = (
            "PostFixedF12(",
            "PostMessage(",
            "VK_F12",
            "sendNativeDiscordQaAtomicText(",
            "send_native_discord_qa_probe",
        )
        for needle in forbidden_live_mechanisms:
            if needle in source:
                raise AssertionError(f"QA-only live mechanism is present: {needle}")
        approval_at = source.index("Assert-LiveDriveApproval")
        launch_at = source.index("Start-Process -FilePath $exactExe")
        drive_body_at = source.index("function Drive-ApprovedShippingSend")
        approval_call_at = source.index("Assert-LiveDriveApproval", drive_body_at)
        if not (approval_at < drive_body_at < approval_call_at < launch_at):
            raise AssertionError("owner approval must dominate every process launch")

    def test_shipping_harness_uses_only_real_ui_send_path(self) -> None:
        source = (HERE / "osl-local-discord-fixed-probe.ps1").read_text(
            encoding="utf-8"
        )
        self.assert_contract(source)

    def test_removing_owner_gate_or_real_button_drive_breaks_contract(self) -> None:
        source = (HERE / "osl-local-discord-fixed-probe.ps1").read_text(
            encoding="utf-8"
        )
        for mutation in (
            source.replace("if (-not $ConfirmOwnerApprovedSend)", "if ($false)", 1),
            source.replace("Invoke-UiaButton $send", "# removed", 1),
        ):
            with self.assertRaises(AssertionError):
                self.assert_contract(mutation)


class DiscordReleaseQualificationReportTests(unittest.TestCase):
    REQUIRED_PHRASES = (
        "## Discord Release Qualification {#discord_release_qualification}",
        "`discord_release_qualification: test-proven-only`",
        "npm test -- overlay-send-gesture.test.ts discord-qa-send-stage.ts",
        "python3 scripts/qa/c4-shipping-evidence-test.py",
        "first trusted Enter is consumed",
        "second Enter must be a distinct trusted press after key-up",
        "intervening draft input or an invalid second Enter attempt cancels",
        "no path auto-retries",
        "osl-c4-shipping-evidence-v1",
        "npm --prefix apps/osl-hub-ui run build",
        "osl-cargo",
        '["desktop"]',
        "`qaShell` is `false`",
        "send_native_discord_qa_atomic_text",
        "discord-qa-send-stage-receipt.json",
        "osl-discord-qa-send-stage.txt",
        "production-overlay-ui",
        "shipping-renderer-success-gate",
        "prepare-protected",
        "send_native_discord_overlay_carrier",
        'status: "sent"',
        "`placed: true`",
        "`enterSent: true`",
        "native-pre-enter-exact-readback",
        "`rawExact`",
        "`utf8Bytes: 0`",
        "empty SHA-256",
        "row count increases by exactly one",
        "`newRows` contains exactly one row",
        "screenshot is a PNG inside the evidence bundle",
        "QA-shell trails, QA atomic command receipts",
        "owner-approved run against the exact shipping executable",
    )

    @classmethod
    def assert_release_qualification(cls, report: str) -> None:
        normalized = " ".join(report.split())
        for phrase in cls.REQUIRED_PHRASES:
            if " ".join(phrase.split()) not in normalized:
                raise AssertionError(f"release qualification report is missing: {phrase}")
        section_start = report.index(
            "## Discord Release Qualification {#discord_release_qualification}"
        )
        section = report[section_start:]
        if "release-qualified" in section or "runtime-proven" in section:
            raise AssertionError("local C4/Double Enter evidence must not be promoted")
        if "QA-shell trails, QA atomic command receipts" not in section:
            raise AssertionError("QA-only send evidence must be explicitly inadmissible")

    def test_discord_release_qualification_publishes_production_only_evidence(self) -> None:
        report = RELEASE_REPORT.read_text(encoding="utf-8")
        self.assert_release_qualification(report)

    def test_missing_production_receipt_or_double_enter_boundary_breaks_report(self) -> None:
        report = RELEASE_REPORT.read_text(encoding="utf-8")
        for mutation in (
            report.replace(
                "send_native_discord_overlay_carrier",
                "send_native_discord_qa_atomic_text",
            ),
            report.replace("distinct trusted press", "trusted press", 1),
            report.replace(
                "QA-shell trails, QA atomic command receipts",
                "QA-shell trails",
                1,
            ),
        ):
            with self.assertRaises(AssertionError):
                self.assert_release_qualification(mutation)


if __name__ == "__main__":
    unittest.main(verbosity=2)
