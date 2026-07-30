#!/usr/bin/env python3
"""Mutation tests for accessibility bench redaction refusal boundaries."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("a11ybench_redaction", HERE / "redaction.py")
assert SPEC and SPEC.loader
REDACTION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REDACTION)


SHA = "a" * 64


def positive_evidence() -> dict:
    return {
        "schema": "osl-a11y-bench-evidence-v1",
        "benchId": "native-visible-row-accessibility",
        "runId": "018f2c18-59e3-7cc6-98cf-6f3132439850",
        "createdAtUnixMs": 1772110800000,
        "subject": {
            "platform": "windows",
            "appBuildSha256": SHA,
            "executableSha256": "b" * 64,
            "surface": "conversation-view",
        },
        "consent": {
            "granted": True,
            "source": "local-manual-qa-run",
            "observedAtUnixMs": 1772110799000,
        },
        "binding": {
            "method": "launched-subject-process",
            "subjectBindingSha256": "c" * 64,
            "observedAtUnixMs": 1772110799500,
        },
        "authority": {
            "collector": "windows-uia-read-only",
            "verifier": "independent-host-gate",
            "observedAtUnixMs": 1772110800100,
        },
        "measurements": [
            {
                "name": "visible-row-count",
                "status": "pass",
                "observedAtUnixMs": 1772110800200,
                "facts": {
                    "rowsObserved": 3,
                    "ownRows": 1,
                    "peerRows": 1,
                    "unknownRows": 1,
                },
            }
        ],
        "artifacts": [
            {
                "kind": "structured-json",
                "relativePath": "a11ybench/facts.json",
                "sha256": "d" * 64,
                "byteLength": 2048,
                "containsUserContent": False,
            }
        ],
        "privacy": {
            "noRawText": True,
            "noAccountIdentifiers": True,
            "noSecrets": True,
            "boundedArtifacts": True,
        },
        "verdict": "pass",
    }


class AccessibilityBenchRedactionTests(unittest.TestCase):
    def reject(self, mutation) -> str:
        evidence = positive_evidence()
        mutation(evidence)
        with self.assertRaises(REDACTION.RedactionError) as raised:
            REDACTION.reject_content_fields(evidence)
        return str(raised.exception)

    def test_complete_non_content_evidence_is_allowed(self) -> None:
        self.assertIsNone(REDACTION.reject_content_fields(positive_evidence()))

    def test_artifact_marked_as_user_content_is_rejected(self) -> None:
        reason = self.reject(
            lambda evidence: evidence["artifacts"][0].update(
                containsUserContent=True
            )
        )
        self.assertIn("containsUserContent", reason)

    def test_artifact_without_content_declaration_is_rejected(self) -> None:
        reason = self.reject(
            lambda evidence: evidence["artifacts"][0].pop("containsUserContent")
        )
        self.assertIn("artifact content declaration", reason)

    def test_nested_raw_text_field_is_rejected_without_echoing_value(self) -> None:
        secret_text = "alice@example.test sent the recovery phrase"
        reason = self.reject(
            lambda evidence: evidence["measurements"][0]["facts"].update(
                rawText=secret_text
            )
        )
        self.assertIn("content-bearing evidence field", reason)
        self.assertNotIn("alice@example.test", reason)
        self.assertNotIn("recovery phrase", reason)

    def test_derived_content_field_is_rejected(self) -> None:
        reason = self.reject(
            lambda evidence: evidence["artifacts"][0].update(
                messageContentSha256="e" * 64
            )
        )
        self.assertIn("content-bearing evidence field", reason)
        self.assertNotIn("e" * 64, reason)

    def test_nested_account_and_credential_fields_are_rejected(self) -> None:
        for field in ("accountIdentifier", "windowHandle", "credential"):
            with self.subTest(field=field):
                reason = self.reject(
                    lambda evidence, field=field: evidence["measurements"][0][
                        "facts"
                    ].update({field: "sensitive"})
                )
                self.assertIn("content-bearing evidence field", reason)
                self.assertNotIn("sensitive", reason)

    def test_privacy_flags_are_required_true(self) -> None:
        false_reason = self.reject(
            lambda evidence: evidence["privacy"].update(noSecrets=False)
        )
        self.assertIn("privacy.noSecrets", false_reason)

        missing_reason = self.reject(
            lambda evidence: evidence["privacy"].pop("noRawText")
        )
        self.assertIn("privacy evidence is incomplete", missing_reason)

    def test_missing_or_denied_consent_is_rejected(self) -> None:
        missing_reason = self.reject(lambda evidence: evidence.pop("consent"))
        self.assertIn("required refusal boundary", missing_reason)

        denied_reason = self.reject(
            lambda evidence: evidence["consent"].update(granted=False)
        )
        self.assertIn("consent.granted", denied_reason)

    def test_missing_binding_or_authority_is_rejected(self) -> None:
        binding_reason = self.reject(lambda evidence: evidence.pop("binding"))
        self.assertIn("required refusal boundary", binding_reason)

        authority_reason = self.reject(
            lambda evidence: evidence["authority"].pop("verifier")
        )
        self.assertIn("authority evidence is incomplete", authority_reason)


def _reject_content_bearing_accessibility_bench_artifacts(
    self: AccessibilityBenchRedactionTests,
) -> None:
    self.assertIsNone(REDACTION.reject_content_fields(positive_evidence()))

    content_text = "user-visible row text must not survive"
    reason = self.reject(
        lambda evidence: evidence["artifacts"][0].update(
            containsUserContent=True,
            transcriptText=content_text,
        )
    )
    self.assertIn("containsUserContent", reason)
    self.assertNotIn(content_text, reason)

    screenshot_path = "a11ybench/visible-user-row.png"
    reason = self.reject(
        lambda evidence: evidence["artifacts"][0].update(
            kind="screenshot-png",
            relativePath=screenshot_path,
            containsUserContent=False,
        )
    )
    self.assertIn("content-bearing artifact", reason)
    self.assertNotIn(screenshot_path, reason)

    transcript_path = "a11ybench/transcript.json"
    reason = self.reject(
        lambda evidence: evidence["artifacts"][0].update(
            kind="structured-json",
            relativePath=transcript_path,
            containsUserContent=False,
        )
    )
    self.assertIn("content-bearing artifact", reason)
    self.assertNotIn(transcript_path, reason)

    reason = self.reject(
        lambda evidence: evidence["artifacts"][0].update(
            messageContentSha256="e" * 64
        )
    )
    self.assertIn("content-bearing evidence field", reason)
    self.assertNotIn("e" * 64, reason)


setattr(
    AccessibilityBenchRedactionTests,
    "Reject content-bearing accessibility bench artifacts",
    _reject_content_bearing_accessibility_bench_artifacts,
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    tests.addTest(
        AccessibilityBenchRedactionTests(
            "Reject content-bearing accessibility bench artifacts"
        )
    )
    return tests


if __name__ == "__main__":
    unittest.main(verbosity=2)
